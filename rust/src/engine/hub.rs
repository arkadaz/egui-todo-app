//! All of Focus Hub's logic, with no UI code. The Flutter bridge (`crate::api`) is a thin
//! layer on top of this. Every method takes the current time as a parameter, so the tests
//! can control the clock.

use anyhow::{bail, Context, Result};
use chrono::{Datelike, NaiveDate};
use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use super::dates::month_key;
use super::domain::{AppData, Reward, TodoItem};
use super::persistence::JsonStore;
use super::timer::{Completion, StudyTimer, TimerMode};

/// Save a running timer's study time at least this often.
const SAVE_EVERY_MS: i64 = 15_000;
const MAX_TEXT_CHARS: usize = 200;
const MAX_WORK_SECS: u64 = 120 * 60 + 59;
const MAX_BREAK_SECS: u64 = 60 * 60 + 59;
const MAX_LOOPS: u32 = 20;
const BACKGROUND_EXTENSIONS: [&str; 5] = ["gif", "png", "jpg", "jpeg", "webp"];
const BACKGROUNDS_DIR: &str = "backgrounds";

pub struct Hub {
    data: AppData,
    store: JsonStore,
    data_dir: PathBuf,
    timer: StudyTimer,
    /// Study milliseconds not yet added to the stats (they're added in whole seconds).
    study_carry_ms: u64,
    last_save_ms: i64,
    /// Sessions that ended while the app was closed, reported on the first tick.
    pending_completions: Vec<Completion>,
    load_warning: Option<String>,
}

/// Counts after an import.
pub struct Imported {
    pub days: u32,
    pub tasks: u32,
    pub rewards: u32,
}

impl Hub {
    /// Opens (or creates) the data in `data_dir`. A timer that was running when the app
    /// closed catches up on the time that passed.
    pub fn open(data_dir: &Path, now_ms: i64, today: NaiveDate) -> Result<Self> {
        fs::create_dir_all(data_dir)
            .with_context(|| format!("Couldn't create the data folder {}", data_dir.display()))?;
        let store = JsonStore::in_dir(data_dir);
        let (mut data, load_warning) = store.load(now_ms)?;
        clean_up(&mut data);
        let settings = data.settings.clone();
        let mut timer = StudyTimer::new(
            Duration::from_secs(settings.work_secs),
            Duration::from_secs(settings.break_secs),
            settings.loops,
        );
        if let Some(saved) = &data.timer {
            timer.restore(saved);
        }
        let mut hub = Self {
            data,
            store,
            data_dir: data_dir.to_path_buf(),
            timer,
            study_carry_ms: 0,
            last_save_ms: now_ms,
            pending_completions: Vec::new(),
            load_warning,
        };
        let outcome = hub.timer.advance(now_ms);
        hub.pending_completions = hub.apply(outcome, today);
        hub.save(now_ms)?;
        Ok(hub)
    }

    pub fn load_warning(&self) -> Option<String> {
        self.load_warning.clone()
    }

    pub fn data_file(&self) -> String {
        self.store.path().display().to_string()
    }

    pub fn save(&mut self, now_ms: i64) -> Result<()> {
        self.data.timer = Some(self.timer.save());
        self.store.save(&self.data)?;
        self.last_save_ms = now_ms;
        Ok(())
    }

    // ---- Timer --------------------------------------------------------------------------

    pub fn timer(&self) -> &StudyTimer {
        &self.timer
    }

    /// Brings the timer up to date. Returns the sessions that ended.
    pub fn tick(&mut self, now_ms: i64, today: NaiveDate) -> Result<Vec<Completion>> {
        let outcome = self.timer.advance(now_ms);
        let mut ended = std::mem::take(&mut self.pending_completions);
        ended.extend(self.apply(outcome, today));
        let save_due = self.timer.is_running() && now_ms - self.last_save_ms >= SAVE_EVERY_MS;
        if !ended.is_empty() || save_due {
            self.save(now_ms)?;
        }
        Ok(ended)
    }

    /// Start or pause. Pausing counts the time up to now first.
    pub fn toggle_timer(&mut self, now_ms: i64, today: NaiveDate) -> Result<Vec<Completion>> {
        let outcome = self.timer.toggle(now_ms);
        let ended = self.apply(outcome, today);
        self.save(now_ms)?;
        Ok(ended)
    }

    pub fn reset_timer(&mut self, now_ms: i64, today: NaiveDate) -> Result<()> {
        // Keep the study time done so far, then reset.
        let outcome = self.timer.advance(now_ms);
        self.apply(outcome, today);
        self.flush_study_carry(today);
        self.timer.reset();
        self.save(now_ms)
    }

    /// Sets the session lengths and loop count (clamped to the desktop app's limits),
    /// which resets the timer.
    pub fn set_timer_settings(
        &mut self,
        work_secs: u64,
        break_secs: u64,
        loops: u32,
        now_ms: i64,
        today: NaiveDate,
    ) -> Result<()> {
        let outcome = self.timer.advance(now_ms);
        self.apply(outcome, today);
        self.flush_study_carry(today);
        let settings = &mut self.data.settings;
        settings.work_secs = work_secs.min(MAX_WORK_SECS);
        settings.break_secs = break_secs.min(MAX_BREAK_SECS);
        settings.loops = loops.clamp(1, MAX_LOOPS);
        self.timer.set_durations(
            Duration::from_secs(settings.work_secs),
            Duration::from_secs(settings.break_secs),
            settings.loops,
        );
        self.save(now_ms)
    }

    pub fn upcoming(&self, now_ms: i64) -> Vec<(Duration, Completion)> {
        self.timer.upcoming(now_ms)
    }

    /// Adds study time and session counts to the stats.
    fn apply(&mut self, outcome: super::timer::TickOutcome, today: NaiveDate) -> Vec<Completion> {
        self.study_carry_ms += outcome.study.as_millis() as u64;
        let whole_seconds = self.study_carry_ms / 1000;
        if whole_seconds > 0 {
            *self.data.stats.daily_study_seconds.entry(today).or_insert(0) += whole_seconds;
            self.study_carry_ms %= 1000;
        }
        for completion in &outcome.completed {
            // Like the desktop app, a session counts once its break is over.
            if completion.mode == TimerMode::Break {
                *self.data.stats.daily_streaks.entry(today).or_insert(0) += 1;
                *self.data.stats.monthly_streaks.entry(month_key(today)).or_insert(0) += 1;
            }
        }
        outcome.completed
    }

    /// Rounds any leftover study milliseconds to the nearest second.
    fn flush_study_carry(&mut self, today: NaiveDate) {
        if self.study_carry_ms >= 500 {
            *self.data.stats.daily_study_seconds.entry(today).or_insert(0) += 1;
        }
        self.study_carry_ms = 0;
    }

    // ---- Clock --------------------------------------------------------------------------

    pub fn gmt_offset_hours(&self) -> Option<i32> {
        self.data.settings.gmt_offset_hours
    }

    /// `None` follows the device's time zone.
    pub fn set_gmt_offset_hours(&mut self, hours: Option<i32>, now_ms: i64) -> Result<()> {
        if let Some(h) = hours {
            if !(-12..=14).contains(&h) {
                bail!("Time zones go from GMT-12 to GMT+14.");
            }
        }
        self.data.settings.gmt_offset_hours = hours;
        self.save(now_ms)
    }

    // ---- To-dos -------------------------------------------------------------------------

    pub fn todos(&self, date: NaiveDate) -> Vec<TodoItem> {
        self.data.todos_by_date.get(&date).cloned().unwrap_or_default()
    }

    pub fn add_todo(&mut self, date: NaiveDate, text: &str, now_ms: i64) -> Result<()> {
        let text = checked_text(text, "a task")?;
        self.data
            .todos_by_date
            .entry(date)
            .or_default()
            .push(TodoItem { text, completed: false });
        self.save(now_ms)
    }

    pub fn set_todo_completed(&mut self, date: NaiveDate, index: usize, completed: bool, now_ms: i64) -> Result<()> {
        let todo = self
            .data
            .todos_by_date
            .get_mut(&date)
            .and_then(|todos| todos.get_mut(index))
            .context("That task no longer exists.")?;
        todo.completed = completed;
        self.save(now_ms)
    }

    pub fn delete_todo(&mut self, date: NaiveDate, index: usize, now_ms: i64) -> Result<()> {
        let todos = self.data.todos_by_date.get_mut(&date).context("That task no longer exists.")?;
        if index >= todos.len() {
            bail!("That task no longer exists.");
        }
        todos.remove(index);
        if todos.is_empty() {
            self.data.todos_by_date.remove(&date);
        }
        self.save(now_ms)
    }

    /// Days before `date` that have tasks, newest first.
    pub fn history_before(&self, date: NaiveDate) -> Vec<(NaiveDate, Vec<TodoItem>)> {
        let days: BTreeMap<_, _> = self
            .data
            .todos_by_date
            .iter()
            .filter(|(day, todos)| **day < date && !todos.is_empty())
            .map(|(day, todos)| (*day, todos.clone()))
            .collect();
        days.into_iter().rev().collect()
    }

    /// Days of the given month that have at least one task.
    pub fn days_with_todos(&self, year: i32, month: u32) -> HashSet<u32> {
        self.data
            .todos_by_date
            .iter()
            .filter(|(day, todos)| day.year() == year && day.month() == month && !todos.is_empty())
            .map(|(day, _)| day.day())
            .collect()
    }

    // ---- Rewards ------------------------------------------------------------------------

    /// Rewards, unfinished ones first (like the desktop app).
    pub fn rewards(&self) -> &[Reward] {
        &self.data.rewards
    }

    pub fn add_reward(&mut self, name: &str, now_ms: i64) -> Result<()> {
        let name = checked_text(name, "a reward")?;
        self.data.rewards.push(Reward { name, completed: false });
        sort_rewards(&mut self.data.rewards);
        self.save(now_ms)
    }

    pub fn set_reward_completed(&mut self, index: usize, completed: bool, now_ms: i64) -> Result<()> {
        let reward = self.data.rewards.get_mut(index).context("That reward no longer exists.")?;
        reward.completed = completed;
        sort_rewards(&mut self.data.rewards);
        self.save(now_ms)
    }

    pub fn delete_reward(&mut self, index: usize, now_ms: i64) -> Result<()> {
        if index >= self.data.rewards.len() {
            bail!("That reward no longer exists.");
        }
        self.data.rewards.remove(index);
        self.save(now_ms)
    }

    // ---- Stats --------------------------------------------------------------------------

    /// Includes study time that hasn't been saved yet, so the numbers move live.
    pub fn total_study_seconds(&self) -> u64 {
        self.data.stats.daily_study_seconds.values().sum()
    }

    pub fn study_seconds_on(&self, day: NaiveDate) -> u64 {
        self.data.stats.daily_study_seconds.get(&day).copied().unwrap_or(0)
    }

    pub fn sessions_on(&self, day: NaiveDate) -> u32 {
        self.data.stats.daily_streaks.get(&day).copied().unwrap_or(0)
    }

    pub fn sessions_in_month_of(&self, day: NaiveDate) -> u32 {
        self.data.stats.monthly_streaks.get(&month_key(day)).copied().unwrap_or(0)
    }

    // ---- Background ---------------------------------------------------------------------

    /// The custom background image, if one is set and still exists.
    pub fn background_path(&self) -> Option<String> {
        self.data
            .gif_path
            .as_ref()
            .filter(|path| Path::new(path).is_file())
            .cloned()
    }

    /// Stores a copy of the chosen image in the app's own folder, so it keeps working even
    /// if the original file is moved or deleted.
    pub fn set_background(&mut self, file_name: &str, bytes: &[u8], now_ms: i64) -> Result<String> {
        let extension = Path::new(file_name)
            .extension()
            .map(|ext| ext.to_string_lossy().to_lowercase())
            .unwrap_or_default();
        if !BACKGROUND_EXTENSIONS.contains(&extension.as_str()) {
            bail!("Please choose a GIF, PNG, JPG or WebP image.");
        }
        if bytes.is_empty() {
            bail!("That file is empty.");
        }
        let dir = self.data_dir.join(BACKGROUNDS_DIR);
        fs::create_dir_all(&dir)?;
        // A new name each time, so the UI never shows a cached old image.
        let path = dir.join(format!("background-{now_ms}.{extension}"));
        fs::write(&path, bytes).with_context(|| format!("Couldn't save {}", path.display()))?;
        self.remove_custom_background();
        self.data.gif_path = Some(path.display().to_string());
        self.save(now_ms)?;
        Ok(path.display().to_string())
    }

    pub fn clear_background(&mut self, now_ms: i64) -> Result<()> {
        self.remove_custom_background();
        self.data.gif_path = None;
        self.save(now_ms)
    }

    /// Deletes the current background, but only if it's our own copy.
    fn remove_custom_background(&self) {
        if let Some(old) = &self.data.gif_path {
            let old = Path::new(old);
            if old.starts_with(self.data_dir.join(BACKGROUNDS_DIR)) {
                let _ = fs::remove_file(old);
            }
        }
    }

    // ---- Import / export ----------------------------------------------------------------

    pub fn export_json(&mut self, now_ms: i64) -> Result<String> {
        self.save(now_ms)?;
        Ok(serde_json::to_string_pretty(&self.data)?)
    }

    /// Replaces the tasks, stats and rewards with those from a `focushub_data.json` file
    /// (from this app or the desktop app). Settings, the timer and the background stay.
    pub fn import_json(&mut self, json: &str, now_ms: i64) -> Result<Imported> {
        let mut imported: AppData = serde_json::from_str(json)
            .context("That file isn't Focus Hub data (focushub_data.json).")?;
        clean_up(&mut imported);
        self.data.todos_by_date = imported.todos_by_date;
        self.data.stats = imported.stats;
        self.data.rewards = imported.rewards;
        self.save(now_ms)?;
        Ok(Imported {
            days: self.data.todos_by_date.len() as u32,
            tasks: self.data.todos_by_date.values().map(Vec::len).sum::<usize>() as u32,
            rewards: self.data.rewards.len() as u32,
        })
    }
}

/// Trims text and checks it isn't empty or too long.
fn checked_text(text: &str, what: &str) -> Result<String> {
    let text = text.trim();
    if text.is_empty() {
        bail!("Type {what} first.");
    }
    if text.chars().count() > MAX_TEXT_CHARS {
        bail!("That's too long ({MAX_TEXT_CHARS} characters at most).");
    }
    Ok(text.to_string())
}

fn sort_rewards(rewards: &mut [Reward]) {
    rewards.sort_by_key(|reward| reward.completed);
}

/// Tidies data from any source: drops empty days, sorts rewards.
fn clean_up(data: &mut AppData) {
    data.todos_by_date.retain(|_, todos| !todos.is_empty());
    sort_rewards(&mut data.rewards);
}

#[cfg(test)]
mod tests {
    use super::*;

    const SEC: i64 = 1000;

    fn day(d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 9, d).unwrap()
    }

    fn open(dir: &Path) -> Hub {
        Hub::open(dir, 0, day(26)).unwrap()
    }

    #[test]
    fn todos_add_toggle_delete_and_persist() {
        let dir = tempfile::tempdir().unwrap();
        let mut hub = open(dir.path());
        hub.add_todo(day(26), "  Read chapter 3  ", 0).unwrap();
        hub.add_todo(day(26), "Practice", 0).unwrap();
        assert!(hub.add_todo(day(26), "   ", 0).is_err());
        assert!(hub.add_todo(day(26), &"x".repeat(201), 0).is_err());
        hub.set_todo_completed(day(26), 0, true, 0).unwrap();
        hub.delete_todo(day(26), 1, 0).unwrap();
        assert!(hub.delete_todo(day(26), 5, 0).is_err());

        let reopened = open(dir.path());
        assert_eq!(
            reopened.todos(day(26)),
            vec![TodoItem { text: "Read chapter 3".into(), completed: true }]
        );
    }

    #[test]
    fn deleting_the_last_task_removes_the_day() {
        let dir = tempfile::tempdir().unwrap();
        let mut hub = open(dir.path());
        hub.add_todo(day(26), "Only one", 0).unwrap();
        hub.delete_todo(day(26), 0, 0).unwrap();
        assert!(hub.days_with_todos(2026, 9).is_empty());
    }

    #[test]
    fn history_lists_earlier_days_newest_first() {
        let dir = tempfile::tempdir().unwrap();
        let mut hub = open(dir.path());
        hub.add_todo(day(20), "A", 0).unwrap();
        hub.add_todo(day(24), "B", 0).unwrap();
        hub.add_todo(day(26), "Today", 0).unwrap();
        hub.add_todo(day(28), "Later", 0).unwrap();
        let days: Vec<_> = hub.history_before(day(26)).into_iter().map(|(d, _)| d).collect();
        assert_eq!(days, vec![day(24), day(20)]);
        assert_eq!(hub.days_with_todos(2026, 9), HashSet::from([20, 24, 26, 28]));
        assert!(hub.days_with_todos(2026, 10).is_empty());
    }

    #[test]
    fn rewards_keep_unfinished_first() {
        let dir = tempfile::tempdir().unwrap();
        let mut hub = open(dir.path());
        hub.add_reward("Cake", 0).unwrap();
        hub.add_reward("Movie", 0).unwrap();
        hub.set_reward_completed(0, true, 0).unwrap(); // Cake done
        let names: Vec<_> = hub.rewards().iter().map(|r| r.name.as_str()).collect();
        assert_eq!(names, vec!["Movie", "Cake"]);
        hub.delete_reward(1, 0).unwrap();
        assert_eq!(hub.rewards().len(), 1);
        assert!(hub.set_reward_completed(9, true, 0).is_err());
    }

    #[test]
    fn timer_records_study_time_and_sessions() {
        let dir = tempfile::tempdir().unwrap();
        let mut hub = open(dir.path());
        hub.set_timer_settings(10, 5, 1, 0, day(26)).unwrap();
        hub.toggle_timer(0, day(26)).unwrap();
        assert!(hub.tick(4 * SEC + 500, day(26)).unwrap().is_empty());
        assert_eq!(hub.study_seconds_on(day(26)), 4);

        let ended = hub.tick(15 * SEC, day(26)).unwrap();
        let modes: Vec<_> = ended.iter().map(|c| c.mode).collect();
        assert_eq!(modes, vec![TimerMode::Work, TimerMode::Break]);
        assert_eq!(hub.study_seconds_on(day(26)), 10);
        assert_eq!(hub.sessions_on(day(26)), 1);
        assert_eq!(hub.sessions_in_month_of(day(26)), 1);
        assert!(!hub.timer().is_running());
    }

    #[test]
    fn a_running_timer_survives_closing_the_app() {
        let dir = tempfile::tempdir().unwrap();
        {
            let mut hub = open(dir.path());
            hub.set_timer_settings(60, 30, 1, 0, day(26)).unwrap();
            hub.toggle_timer(0, day(26)).unwrap();
            hub.tick(10 * SEC, day(26)).unwrap();
        } // app closed 10s into the work session

        // Reopened 70s later: work finished (50s more study), break is 20s in.
        let mut hub = Hub::open(dir.path(), 80 * SEC, day(26)).unwrap();
        assert!(hub.timer().is_running());
        assert_eq!(hub.timer().timer_mode, TimerMode::Break);
        assert_eq!(hub.study_seconds_on(day(26)), 60);
        let ended = hub.tick(80 * SEC, day(26)).unwrap();
        assert_eq!(ended.len(), 1, "the work session that ended while closed is reported");
        assert!(hub.tick(81 * SEC, day(26)).unwrap().is_empty(), "...only once");
    }

    #[test]
    fn settings_are_clamped_and_saved() {
        let dir = tempfile::tempdir().unwrap();
        let mut hub = open(dir.path());
        hub.set_timer_settings(99_999, 99_999, 99, 0, day(26)).unwrap();
        let reopened = open(dir.path());
        let t = reopened.timer();
        assert_eq!(t.work_duration.as_secs(), MAX_WORK_SECS);
        assert_eq!(t.break_duration.as_secs(), MAX_BREAK_SECS);
        assert_eq!(t.total_loops, MAX_LOOPS);

        hub.set_timer_settings(10, 5, 0, 0, day(26)).unwrap();
        assert_eq!(hub.timer().total_loops, 1);
    }

    #[test]
    fn reset_keeps_the_study_time_so_far() {
        let dir = tempfile::tempdir().unwrap();
        let mut hub = open(dir.path());
        hub.set_timer_settings(100, 5, 1, 0, day(26)).unwrap();
        hub.toggle_timer(0, day(26)).unwrap();
        hub.reset_timer(7 * SEC + 600, day(26)).unwrap();
        assert_eq!(hub.study_seconds_on(day(26)), 8);
        assert!(!hub.timer().is_running());
        assert_eq!(hub.timer().time_remaining.as_secs(), 100);
    }

    #[test]
    fn time_zone_setting_is_validated() {
        let dir = tempfile::tempdir().unwrap();
        let mut hub = open(dir.path());
        hub.set_gmt_offset_hours(Some(7), 0).unwrap();
        assert!(hub.set_gmt_offset_hours(Some(15), 0).is_err());
        assert_eq!(open(dir.path()).gmt_offset_hours(), Some(7));
        hub.set_gmt_offset_hours(None, 0).unwrap();
        assert_eq!(open(dir.path()).gmt_offset_hours(), None);
    }

    #[test]
    fn backgrounds_are_copied_and_replaced() {
        let dir = tempfile::tempdir().unwrap();
        let mut hub = open(dir.path());
        assert!(hub.set_background("notes.txt", b"hi", 1).is_err());
        let first = hub.set_background("Duck.GIF", b"GIF89a...", 1).unwrap();
        assert_eq!(hub.background_path(), Some(first.clone()));
        let second = hub.set_background("cat.png", b"PNG...", 2).unwrap();
        assert!(!Path::new(&first).exists(), "the old copy is deleted");
        assert_eq!(open(dir.path()).background_path(), Some(second.clone()));
        hub.clear_background(3).unwrap();
        assert!(hub.background_path().is_none());
        assert!(!Path::new(&second).exists());
    }

    #[test]
    fn a_missing_background_file_falls_back() {
        let dir = tempfile::tempdir().unwrap();
        let mut hub = open(dir.path());
        hub.data.gif_path = Some("C:\\Users\\someone\\duck.gif".into());
        assert!(hub.background_path().is_none());
        // Clearing a path we don't own must not try to delete it.
        hub.clear_background(0).unwrap();
    }

    #[test]
    fn import_replaces_data_but_keeps_settings() {
        let dir = tempfile::tempdir().unwrap();
        let mut hub = open(dir.path());
        hub.set_timer_settings(25 * 60, 5 * 60, 4, 0, day(26)).unwrap();
        hub.add_todo(day(26), "Old task", 0).unwrap();
        let desktop_json = r#"{
            "todos_by_date": {
                "2025-07-25": [ { "text": "A", "completed": false }, { "text": "B", "completed": true } ],
                "2025-07-26": []
            },
            "stats": { "daily_study_seconds": { "2025-07-25": 100 }, "daily_streaks": {}, "monthly_streaks": {} },
            "rewards": [ { "name": "Done", "completed": true }, { "name": "Open", "completed": false } ]
        }"#;
        let imported = hub.import_json(desktop_json, 0).unwrap();
        assert_eq!((imported.days, imported.tasks, imported.rewards), (1, 2, 2));
        assert!(hub.todos(day(26)).is_empty());
        assert_eq!(hub.rewards()[0].name, "Open", "rewards are sorted after import");
        assert_eq!(hub.timer().total_loops, 4, "settings are kept");
        assert!(hub.import_json("not json", 0).is_err());
    }

    #[test]
    fn export_round_trips_through_import() {
        let dir = tempfile::tempdir().unwrap();
        let mut hub = open(dir.path());
        hub.add_todo(day(26), "Keep me", 0).unwrap();
        hub.add_reward("Prize", 0).unwrap();
        let json = hub.export_json(0).unwrap();

        let other = tempfile::tempdir().unwrap();
        let mut hub2 = open(other.path());
        hub2.import_json(&json, 0).unwrap();
        assert_eq!(hub2.todos(day(26))[0].text, "Keep me");
        assert_eq!(hub2.rewards()[0].name, "Prize");
    }

    #[test]
    fn a_damaged_file_opens_with_a_warning() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join(super::super::persistence::FILE_NAME), "oops").unwrap();
        let hub = open(dir.path());
        assert!(hub.load_warning().is_some());
    }
}
