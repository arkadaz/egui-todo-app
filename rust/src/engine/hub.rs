//! All of Focus Hub's logic, with no UI code. The Flutter bridge (`crate::api`) is a thin
//! layer on top of this. Every method takes the current time as a parameter, so the tests
//! can control the clock.
//!
//! Two copies of the app can be running at once: the normal app, and a short-lived copy
//! that Android starts when a notification button (Pause, Skip...) is tapped. Each copy has
//! its own `Hub`. Before every change, a `Hub` checks whether the file was changed by the
//! other copy and reloads it if so, so neither overwrites the other's work.

use anyhow::{bail, Context, Result};
use chrono::{Duration as Days, NaiveDate};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use super::dates::month_key;
use super::domain::{AppData, Reward, Settings, TodoItem};
use super::persistence::{DiskStamp, JsonStore};
use super::timer::{Completion, StudyTimer, TimerConfig, TimerMode};

/// Save a running timer's study time at least this often.
const SAVE_EVERY_MS: i64 = 15_000;
const MAX_TEXT_CHARS: usize = 200;
const MAX_WORK_SECS: u64 = 120 * 60 + 59;
const MAX_BREAK_SECS: u64 = 60 * 60 + 59;
const MAX_LOOPS: u32 = 20;
/// A day counts toward a streak after this much study.
pub const STREAK_MIN_SECS: u64 = 60;
const BACKGROUND_EXTENSIONS: [&str; 5] = ["gif", "png", "jpg", "jpeg", "webp"];
const BACKGROUNDS_DIR: &str = "backgrounds";

/// Ready-made timer setups.
pub struct Preset {
    pub name: &'static str,
    pub settings: TimerSettings,
}

pub const PRESETS: [Preset; 3] = [
    Preset {
        name: "Classic Pomodoro · 25/5 ×4",
        settings: TimerSettings {
            work_secs: 25 * 60,
            break_secs: 5 * 60,
            long_break_secs: 15 * 60,
            loops: 4,
            long_break_every: 4,
            auto_start: true,
        },
    },
    Preset {
        name: "Deep work · 50/10 ×2",
        settings: TimerSettings {
            work_secs: 50 * 60,
            break_secs: 10 * 60,
            long_break_secs: 15 * 60,
            loops: 2,
            long_break_every: 0,
            auto_start: true,
        },
    },
    Preset {
        name: "Desktop default · 60/5 ×1",
        settings: TimerSettings {
            work_secs: 60 * 60,
            break_secs: 5 * 60,
            long_break_secs: 15 * 60,
            loops: 1,
            long_break_every: 0,
            auto_start: true,
        },
    },
];

/// The timer part of the settings.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TimerSettings {
    pub work_secs: u64,
    pub break_secs: u64,
    pub long_break_secs: u64,
    pub loops: u32,
    pub long_break_every: u32,
    pub auto_start: bool,
}

impl TimerSettings {
    fn of(settings: &Settings) -> Self {
        Self {
            work_secs: settings.work_secs,
            break_secs: settings.break_secs,
            long_break_secs: settings.long_break_secs,
            loops: settings.loops,
            long_break_every: settings.long_break_every,
            auto_start: settings.auto_start,
        }
    }

    fn clamped(self) -> Self {
        let loops = self.loops.clamp(1, MAX_LOOPS);
        Self {
            work_secs: self.work_secs.min(MAX_WORK_SECS),
            break_secs: self.break_secs.min(MAX_BREAK_SECS),
            long_break_secs: self.long_break_secs.min(MAX_BREAK_SECS),
            loops,
            long_break_every: self.long_break_every.min(MAX_LOOPS),
            auto_start: self.auto_start,
        }
    }

    fn timer_config(&self) -> TimerConfig {
        TimerConfig {
            work: Duration::from_secs(self.work_secs),
            short_break: Duration::from_secs(self.break_secs),
            long_break: Duration::from_secs(self.long_break_secs),
            total_loops: self.loops,
            long_break_every: self.long_break_every,
            auto_start: self.auto_start,
        }
    }

    /// The preset these settings match, if any.
    pub fn preset_index(&self) -> Option<usize> {
        PRESETS.iter().position(|p| p.settings == *self)
    }
}

pub struct Hub {
    data: AppData,
    store: JsonStore,
    data_dir: PathBuf,
    timer: StudyTimer,
    /// Study milliseconds not yet added to the stats (they're added in whole seconds).
    study_carry_ms: u64,
    last_save_ms: i64,
    /// What the file looked like after our last read or write.
    disk_stamp: Option<DiskStamp>,
    /// Sessions that ended while the app was closed, reported on the first tick.
    pending_completions: Vec<Completion>,
    load_warning: Option<String>,
}

/// Which of the history's tasks to show.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HistoryFilter {
    All,
    Unfinished,
    Done,
}

impl HistoryFilter {
    fn accepts(self, todo: &TodoItem) -> bool {
        match self {
            HistoryFilter::All => true,
            HistoryFilter::Unfinished => !todo.completed,
            HistoryFilter::Done => todo.completed,
        }
    }
}

/// A day in the task history.
#[derive(Debug, PartialEq)]
pub struct HistoryDay {
    pub date: NaiveDate,
    /// The tasks that matched, with their positions in the day's list.
    pub todos: Vec<(usize, TodoItem)>,
    /// Counts for the whole day, matching or not.
    pub done: usize,
    pub total: usize,
}

/// What a tick found.
pub struct Ticked {
    /// Sessions that ended since the last tick.
    pub ended: Vec<Completion>,
    /// The other copy of the app (a notification button) changed the data.
    pub reloaded: bool,
}

/// Counts after a replace-import.
pub struct Imported {
    pub days: u32,
    pub tasks: u32,
    pub rewards: u32,
}

/// Counts after a merge-import.
pub struct Merged {
    pub tasks_added: u32,
    pub study_days_updated: u32,
    pub rewards_added: u32,
}

fn timer_for(data: &AppData) -> StudyTimer {
    let mut timer = StudyTimer::new(TimerSettings::of(&data.settings).timer_config());
    if let Some(saved) = &data.timer {
        timer.restore(saved);
    }
    timer
}

impl Hub {
    /// Opens (or creates) the data in `data_dir`. A timer that was running when the app
    /// closed catches up on the time that passed. Makes the day's backup.
    pub fn open(data_dir: &Path, now_ms: i64, today: NaiveDate) -> Result<Self> {
        fs::create_dir_all(data_dir)
            .with_context(|| format!("Couldn't create the data folder {}", data_dir.display()))?;
        let store = JsonStore::in_dir(data_dir);
        let (mut data, load_warning) = store.load(now_ms)?;
        if load_warning.is_none() {
            // A backup failing must never stop the app from opening.
            let _ = store.back_up_daily(today);
        }
        clean_up(&mut data);
        let timer = timer_for(&data);
        let mut hub = Self {
            data,
            store,
            data_dir: data_dir.to_path_buf(),
            timer,
            study_carry_ms: 0,
            last_save_ms: now_ms,
            disk_stamp: None,
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
        self.disk_stamp = self.store.save(&self.data)?;
        self.last_save_ms = now_ms;
        Ok(())
    }

    /// Saves (when the app is hidden), unless the other copy of the app changed the file
    /// since this one last read or wrote it: then that version is loaded instead of being
    /// overwritten. Returns true if it loaded.
    pub fn save_unless_changed(&mut self, now_ms: i64) -> Result<bool> {
        if self.reload_if_changed(now_ms)? {
            return Ok(true);
        }
        self.save(now_ms)?;
        Ok(false)
    }

    /// Reloads the data if another copy of the app changed the file. Returns true if it did.
    pub fn reload_if_changed(&mut self, now_ms: i64) -> Result<bool> {
        let current = self.store.stamp();
        if current == self.disk_stamp {
            return Ok(false);
        }
        let (mut data, _) = self.store.load(now_ms)?;
        clean_up(&mut data);
        self.timer = timer_for(&data);
        self.data = data;
        self.study_carry_ms = 0;
        self.disk_stamp = self.store.stamp();
        Ok(true)
    }

    // ---- Timer --------------------------------------------------------------------------

    pub fn timer(&self) -> &StudyTimer {
        &self.timer
    }

    pub fn timer_settings(&self) -> TimerSettings {
        TimerSettings::of(&self.data.settings)
    }

    /// Brings the timer up to date.
    pub fn tick(&mut self, now_ms: i64, today: NaiveDate) -> Result<Ticked> {
        let reloaded = self.reload_if_changed(now_ms)?;
        let outcome = self.timer.advance(now_ms);
        let mut ended = std::mem::take(&mut self.pending_completions);
        ended.extend(self.apply(outcome, today));
        let save_due = self.timer.is_running() && now_ms - self.last_save_ms >= SAVE_EVERY_MS;
        if !ended.is_empty() || save_due {
            self.save(now_ms)?;
        }
        Ok(Ticked { ended, reloaded })
    }

    /// Start or pause. Pausing counts the time up to now first.
    pub fn toggle_timer(&mut self, now_ms: i64, today: NaiveDate) -> Result<Vec<Completion>> {
        self.reload_if_changed(now_ms)?;
        let outcome = self.timer.toggle(now_ms);
        let ended = self.apply(outcome, today);
        self.save(now_ms)?;
        Ok(ended)
    }

    /// Ends the current session now and moves to the next one.
    pub fn skip_session(&mut self, now_ms: i64, today: NaiveDate) -> Result<Vec<Completion>> {
        self.reload_if_changed(now_ms)?;
        let outcome = self.timer.skip(now_ms);
        let ended = self.apply(outcome, today);
        self.flush_study_carry(today);
        self.save(now_ms)?;
        Ok(ended)
    }

    pub fn reset_timer(&mut self, now_ms: i64, today: NaiveDate) -> Result<()> {
        self.reload_if_changed(now_ms)?;
        // Keep the study time done so far, then reset.
        let outcome = self.timer.advance(now_ms);
        self.apply(outcome, today);
        self.flush_study_carry(today);
        self.timer.reset();
        self.save(now_ms)
    }

    /// Changes the timer setup (clamped to the desktop app's limits). This resets the timer.
    pub fn set_timer_settings(&mut self, new: TimerSettings, now_ms: i64, today: NaiveDate) -> Result<()> {
        self.reload_if_changed(now_ms)?;
        let outcome = self.timer.advance(now_ms);
        self.apply(outcome, today);
        self.flush_study_carry(today);
        let new = new.clamped();
        let settings = &mut self.data.settings;
        settings.work_secs = new.work_secs;
        settings.break_secs = new.break_secs;
        settings.long_break_secs = new.long_break_secs;
        settings.loops = new.loops;
        settings.long_break_every = new.long_break_every;
        settings.auto_start = new.auto_start;
        self.timer.configure(new.timer_config());
        self.save(now_ms)
    }

    pub fn apply_preset(&mut self, index: usize, now_ms: i64, today: NaiveDate) -> Result<()> {
        let preset = PRESETS.get(index).context("That preset doesn't exist.")?;
        self.set_timer_settings(preset.settings, now_ms, today)
    }

    pub fn upcoming(&self, now_ms: i64) -> Vec<(Duration, Completion, StudyTimer)> {
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
        self.reload_if_changed(now_ms)?;
        self.data.settings.gmt_offset_hours = hours;
        self.save(now_ms)
    }

    // ---- To-dos -------------------------------------------------------------------------

    pub fn todos(&self, date: NaiveDate) -> Vec<TodoItem> {
        self.data.todos_by_date.get(&date).cloned().unwrap_or_default()
    }

    pub fn add_todo(&mut self, date: NaiveDate, text: &str, now_ms: i64) -> Result<()> {
        let text = checked_text(text, "a task")?;
        self.reload_if_changed(now_ms)?;
        self.data
            .todos_by_date
            .entry(date)
            .or_default()
            .push(TodoItem { text, completed: false });
        self.save(now_ms)
    }

    pub fn set_todo_completed(&mut self, date: NaiveDate, index: usize, completed: bool, now_ms: i64) -> Result<()> {
        self.reload_if_changed(now_ms)?;
        self.todo_mut(date, index)?.completed = completed;
        self.save(now_ms)
    }

    pub fn edit_todo(&mut self, date: NaiveDate, index: usize, text: &str, now_ms: i64) -> Result<()> {
        let text = checked_text(text, "a task")?;
        self.reload_if_changed(now_ms)?;
        self.todo_mut(date, index)?.text = text;
        self.save(now_ms)
    }

    fn todo_mut(&mut self, date: NaiveDate, index: usize) -> Result<&mut TodoItem> {
        self.data
            .todos_by_date
            .get_mut(&date)
            .and_then(|todos| todos.get_mut(index))
            .context("That task no longer exists.")
    }

    /// Deletes a task and returns it (so it can be restored with `restore_todo`).
    pub fn delete_todo(&mut self, date: NaiveDate, index: usize, now_ms: i64) -> Result<TodoItem> {
        self.reload_if_changed(now_ms)?;
        let todos = self.data.todos_by_date.get_mut(&date).context("That task no longer exists.")?;
        if index >= todos.len() {
            bail!("That task no longer exists.");
        }
        let removed = todos.remove(index);
        if todos.is_empty() {
            self.data.todos_by_date.remove(&date);
        }
        self.save(now_ms)?;
        Ok(removed)
    }

    /// Puts a deleted task back where it was (for "Undo").
    pub fn restore_todo(&mut self, date: NaiveDate, index: usize, todo: TodoItem, now_ms: i64) -> Result<()> {
        self.reload_if_changed(now_ms)?;
        let todos = self.data.todos_by_date.entry(date).or_default();
        let index = index.min(todos.len());
        todos.insert(index, todo);
        self.save(now_ms)
    }

    /// Moves a task within its day (for drag-to-reorder). `to` is its new position.
    pub fn move_todo(&mut self, date: NaiveDate, from: usize, to: usize, now_ms: i64) -> Result<()> {
        self.reload_if_changed(now_ms)?;
        let todos = self.data.todos_by_date.get_mut(&date).context("That task no longer exists.")?;
        if from >= todos.len() {
            bail!("That task no longer exists.");
        }
        let todo = todos.remove(from);
        let to = to.min(todos.len());
        todos.insert(to, todo);
        self.save(now_ms)
    }

    /// Days before `date` with tasks that pass `filter` and contain `search` (ignoring
    /// case; empty matches everything), newest first.
    pub fn history(&self, date: NaiveDate, filter: HistoryFilter, search: &str) -> Vec<HistoryDay> {
        let search = search.trim().to_lowercase();
        let mut days: Vec<HistoryDay> = self
            .data
            .todos_by_date
            .iter()
            .filter(|(day, _)| **day < date)
            .filter_map(|(day, todos)| {
                let matching: Vec<(usize, TodoItem)> = todos
                    .iter()
                    .enumerate()
                    .filter(|(_, todo)| filter.accepts(todo))
                    .filter(|(_, todo)| search.is_empty() || todo.text.to_lowercase().contains(&search))
                    .map(|(index, todo)| (index, todo.clone()))
                    .collect();
                (!matching.is_empty()).then(|| HistoryDay {
                    date: *day,
                    todos: matching,
                    done: todos.iter().filter(|todo| todo.completed).count(),
                    total: todos.len(),
                })
            })
            .collect();
        days.sort_by_key(|day| std::cmp::Reverse(day.date));
        days
    }

    /// How many unfinished tasks there are on days before `date`.
    pub fn unfinished_before(&self, date: NaiveDate) -> u32 {
        self.data
            .todos_by_date
            .iter()
            .filter(|(day, _)| **day < date)
            .map(|(_, todos)| todos.iter().filter(|t| !t.completed).count() as u32)
            .sum()
    }

    /// Moves every unfinished task from earlier days onto `date` (oldest first), so
    /// nothing gets lost in the history. Finished tasks stay where they were.
    pub fn move_unfinished_to(&mut self, date: NaiveDate, now_ms: i64) -> Result<u32> {
        self.reload_if_changed(now_ms)?;
        let earlier: Vec<NaiveDate> = {
            let mut days: Vec<_> = self.data.todos_by_date.keys().filter(|d| **d < date).copied().collect();
            days.sort();
            days
        };
        let mut moved = Vec::new();
        for day in earlier {
            if let Some(todos) = self.data.todos_by_date.get_mut(&day) {
                let (unfinished, finished): (Vec<_>, Vec<_>) = todos.drain(..).partition(|t| !t.completed);
                *todos = finished;
                moved.extend(unfinished);
            }
        }
        let count = moved.len() as u32;
        if count > 0 {
            self.data.todos_by_date.entry(date).or_default().extend(moved);
        }
        self.data.todos_by_date.retain(|_, todos| !todos.is_empty());
        self.save(now_ms)?;
        Ok(count)
    }

    /// Days from `first` to `last` that have tasks: `true` if some are still unfinished.
    pub fn task_marks(&self, first: NaiveDate, last: NaiveDate) -> HashMap<NaiveDate, bool> {
        self.data
            .todos_by_date
            .iter()
            .filter(|(day, todos)| (first..=last).contains(*day) && !todos.is_empty())
            .map(|(day, todos)| (*day, todos.iter().any(|todo| !todo.completed)))
            .collect()
    }

    // ---- Rewards ------------------------------------------------------------------------

    /// Rewards, unfinished ones first (like the desktop app).
    pub fn rewards(&self) -> &[Reward] {
        &self.data.rewards
    }

    pub fn add_reward(&mut self, name: &str, now_ms: i64) -> Result<()> {
        let name = checked_text(name, "a reward")?;
        self.reload_if_changed(now_ms)?;
        self.data.rewards.push(Reward { name, completed: false });
        sort_rewards(&mut self.data.rewards);
        self.save(now_ms)
    }

    pub fn set_reward_completed(&mut self, index: usize, completed: bool, now_ms: i64) -> Result<()> {
        self.reload_if_changed(now_ms)?;
        let reward = self.data.rewards.get_mut(index).context("That reward no longer exists.")?;
        reward.completed = completed;
        sort_rewards(&mut self.data.rewards);
        self.save(now_ms)
    }

    pub fn edit_reward(&mut self, index: usize, name: &str, now_ms: i64) -> Result<()> {
        let name = checked_text(name, "a reward")?;
        self.reload_if_changed(now_ms)?;
        self.data.rewards.get_mut(index).context("That reward no longer exists.")?.name = name;
        self.save(now_ms)
    }

    /// Deletes a reward and returns it (so it can be restored with `restore_reward`).
    pub fn delete_reward(&mut self, index: usize, now_ms: i64) -> Result<Reward> {
        self.reload_if_changed(now_ms)?;
        if index >= self.data.rewards.len() {
            bail!("That reward no longer exists.");
        }
        let removed = self.data.rewards.remove(index);
        self.save(now_ms)?;
        Ok(removed)
    }

    /// Puts a deleted reward back (for "Undo").
    pub fn restore_reward(&mut self, index: usize, reward: Reward, now_ms: i64) -> Result<()> {
        self.reload_if_changed(now_ms)?;
        let index = index.min(self.data.rewards.len());
        self.data.rewards.insert(index, reward);
        sort_rewards(&mut self.data.rewards);
        self.save(now_ms)
    }

    // ---- Stats --------------------------------------------------------------------------

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

    /// Study seconds for each of the `days` days ending with `last`, oldest first.
    pub fn study_series(&self, last: NaiveDate, days: u32) -> Vec<(NaiveDate, u64)> {
        (0..days as i64)
            .rev()
            .map(|back| {
                let day = last - Days::days(back);
                (day, self.study_seconds_on(day))
            })
            .collect()
    }

    fn studied(&self, day: NaiveDate) -> bool {
        self.study_seconds_on(day) >= STREAK_MIN_SECS
    }

    /// Days in a row with some study, up to today. Today only counts once you've studied,
    /// so a streak isn't broken just because today hasn't started yet.
    pub fn current_streak(&self, today: NaiveDate) -> u32 {
        let mut day = if self.studied(today) { today } else { today - Days::days(1) };
        let mut streak = 0;
        while self.studied(day) {
            streak += 1;
            day -= Days::days(1);
        }
        streak
    }

    /// The longest run of study days ever.
    pub fn best_streak(&self) -> u32 {
        let mut days: Vec<NaiveDate> = self
            .data
            .stats
            .daily_study_seconds
            .iter()
            .filter(|(_, secs)| **secs >= STREAK_MIN_SECS)
            .map(|(day, _)| *day)
            .collect();
        days.sort();
        let (mut best, mut run) = (0, 0);
        let mut previous: Option<NaiveDate> = None;
        for day in days {
            run = match previous {
                Some(p) if day - p == Days::days(1) => run + 1,
                _ => 1,
            };
            best = best.max(run);
            previous = Some(day);
        }
        best
    }

    /// The day with the most study, if any.
    pub fn best_day(&self) -> Option<(NaiveDate, u64)> {
        self.data
            .stats
            .daily_study_seconds
            .iter()
            .filter(|(_, secs)| **secs > 0)
            .max_by_key(|(day, secs)| (**secs, std::cmp::Reverse(**day)))
            .map(|(day, secs)| (*day, *secs))
    }

    // ---- Background ---------------------------------------------------------------------

    /// Whether the background GIF plays (turning it off saves battery).
    pub fn animate_background(&self) -> bool {
        self.data.settings.animate_background
    }

    pub fn set_animate_background(&mut self, animate: bool, now_ms: i64) -> Result<()> {
        self.reload_if_changed(now_ms)?;
        self.data.settings.animate_background = animate;
        self.save(now_ms)
    }

    /// Whether the app already offered to put its icon on the Home screen.
    pub fn home_icon_offered(&self) -> bool {
        self.data.settings.home_icon_offered
    }

    pub fn set_home_icon_offered(&mut self, now_ms: i64) -> Result<()> {
        self.reload_if_changed(now_ms)?;
        self.data.settings.home_icon_offered = true;
        self.save(now_ms)
    }

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
        self.reload_if_changed(now_ms)?;
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
        self.reload_if_changed(now_ms)?;
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

    // ---- Import / export / backups --------------------------------------------------------

    pub fn export_json(&mut self, now_ms: i64) -> Result<String> {
        self.reload_if_changed(now_ms)?;
        self.save(now_ms)?;
        Ok(serde_json::to_string_pretty(&self.data)?)
    }

    fn parse_import(json: &str) -> Result<AppData> {
        let mut imported: AppData = serde_json::from_str(json)
            .context("That file isn't Focus Hub data (focushub_data.json).")?;
        clean_up(&mut imported);
        Ok(imported)
    }

    /// Replaces the tasks, stats and rewards with those from a `focushub_data.json` file
    /// (from this app or the desktop app). Settings, the timer and the background stay.
    pub fn import_json(&mut self, json: &str, now_ms: i64) -> Result<Imported> {
        let imported = Self::parse_import(json)?;
        self.reload_if_changed(now_ms)?;
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

    /// Combines a `focushub_data.json` file with the data already here, for using the
    /// desktop app and the phone together. Nothing is ever counted twice: tasks and rewards
    /// with the same text are kept once (finished if either copy is), and for each day
    /// the larger study time and session count win. Merging the same file again changes
    /// nothing.
    pub fn merge_json(&mut self, json: &str, now_ms: i64) -> Result<Merged> {
        let imported = Self::parse_import(json)?;
        self.reload_if_changed(now_ms)?;
        let mut merged = Merged { tasks_added: 0, study_days_updated: 0, rewards_added: 0 };

        for (day, todos) in imported.todos_by_date {
            let ours = self.data.todos_by_date.entry(day).or_default();
            for todo in todos {
                match ours.iter_mut().find(|t| t.text == todo.text) {
                    Some(existing) => existing.completed |= todo.completed,
                    None => {
                        ours.push(todo);
                        merged.tasks_added += 1;
                    }
                }
            }
        }

        let stats = &mut self.data.stats;
        for (day, secs) in imported.stats.daily_study_seconds {
            let ours = stats.daily_study_seconds.entry(day).or_insert(0);
            if secs > *ours {
                *ours = secs;
                merged.study_days_updated += 1;
            }
        }
        for (day, count) in imported.stats.daily_streaks {
            let ours = stats.daily_streaks.entry(day).or_insert(0);
            *ours = (*ours).max(count);
        }
        for (month, count) in imported.stats.monthly_streaks {
            let ours = stats.monthly_streaks.entry(month).or_insert(0);
            *ours = (*ours).max(count);
        }

        for reward in imported.rewards {
            match self.data.rewards.iter_mut().find(|r| r.name == reward.name) {
                Some(existing) => existing.completed |= reward.completed,
                None => {
                    self.data.rewards.push(reward);
                    merged.rewards_added += 1;
                }
            }
        }
        sort_rewards(&mut self.data.rewards);
        self.save(now_ms)?;
        Ok(merged)
    }

    /// The dates of the daily backups, newest first.
    pub fn backups(&self) -> Vec<NaiveDate> {
        self.store.backups()
    }

    /// Replaces everything (including settings and the timer) with a daily backup.
    pub fn restore_backup(&mut self, date: NaiveDate, now_ms: i64) -> Result<()> {
        let mut data = self.store.read_backup(date)?;
        clean_up(&mut data);
        self.timer = timer_for(&data);
        self.data = data;
        self.study_carry_ms = 0;
        self.pending_completions.clear();
        self.save(now_ms)
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

    fn settings(work_secs: u64, break_secs: u64, loops: u32) -> TimerSettings {
        TimerSettings {
            work_secs,
            break_secs,
            long_break_secs: break_secs * 3,
            loops,
            long_break_every: 0,
            auto_start: true,
        }
    }

    fn texts(todos: &[TodoItem]) -> Vec<&str> {
        todos.iter().map(|t| t.text.as_str()).collect()
    }

    #[test]
    fn todos_add_toggle_edit_delete_and_persist() {
        let dir = tempfile::tempdir().unwrap();
        let mut hub = open(dir.path());
        hub.add_todo(day(26), "  Read chapter 3  ", 0).unwrap();
        hub.add_todo(day(26), "Practice", 0).unwrap();
        assert!(hub.add_todo(day(26), "   ", 0).is_err());
        assert!(hub.add_todo(day(26), &"x".repeat(201), 0).is_err());
        hub.set_todo_completed(day(26), 0, true, 0).unwrap();
        hub.edit_todo(day(26), 0, " Read chapter 4 ", 0).unwrap();
        assert!(hub.edit_todo(day(26), 0, "", 0).is_err());
        assert!(hub.edit_todo(day(26), 9, "x", 0).is_err());
        let removed = hub.delete_todo(day(26), 1, 0).unwrap();
        assert_eq!(removed.text, "Practice");
        assert!(hub.delete_todo(day(26), 5, 0).is_err());

        let reopened = open(dir.path());
        assert_eq!(
            reopened.todos(day(26)),
            vec![TodoItem { text: "Read chapter 4".into(), completed: true }]
        );
    }

    #[test]
    fn undo_puts_a_deleted_task_back_in_place() {
        let dir = tempfile::tempdir().unwrap();
        let mut hub = open(dir.path());
        for t in ["A", "B", "C"] {
            hub.add_todo(day(26), t, 0).unwrap();
        }
        let removed = hub.delete_todo(day(26), 1, 0).unwrap();
        hub.restore_todo(day(26), 1, removed, 0).unwrap();
        assert_eq!(texts(&hub.todos(day(26))), vec!["A", "B", "C"]);

        // Restoring the only task of a day brings the day back.
        hub.add_todo(day(20), "Only", 0).unwrap();
        let only = hub.delete_todo(day(20), 0, 0).unwrap();
        assert_eq!(hub.task_marks(day(1), day(30)).len(), 1);
        hub.restore_todo(day(20), 5, only, 0).unwrap();
        assert_eq!(texts(&hub.todos(day(20))), vec!["Only"]);
    }

    #[test]
    fn tasks_can_be_reordered() {
        let dir = tempfile::tempdir().unwrap();
        let mut hub = open(dir.path());
        for t in ["A", "B", "C", "D"] {
            hub.add_todo(day(26), t, 0).unwrap();
        }
        hub.move_todo(day(26), 0, 2, 0).unwrap();
        assert_eq!(texts(&hub.todos(day(26))), vec!["B", "C", "A", "D"]);
        hub.move_todo(day(26), 3, 0, 0).unwrap();
        assert_eq!(texts(&hub.todos(day(26))), vec!["D", "B", "C", "A"]);
        hub.move_todo(day(26), 1, 99, 0).unwrap();
        assert_eq!(texts(&hub.todos(day(26))), vec!["D", "C", "A", "B"]);
        assert!(hub.move_todo(day(26), 9, 0, 0).is_err());
    }

    #[test]
    fn deleting_the_last_task_removes_the_day() {
        let dir = tempfile::tempdir().unwrap();
        let mut hub = open(dir.path());
        hub.add_todo(day(26), "Only one", 0).unwrap();
        hub.delete_todo(day(26), 0, 0).unwrap();
        assert!(hub.task_marks(day(1), day(30)).is_empty());
    }

    #[test]
    fn history_lists_earlier_days_newest_first() {
        let dir = tempfile::tempdir().unwrap();
        let mut hub = open(dir.path());
        hub.add_todo(day(20), "A", 0).unwrap();
        hub.add_todo(day(24), "B", 0).unwrap();
        hub.add_todo(day(26), "Today", 0).unwrap();
        hub.add_todo(day(28), "Later", 0).unwrap();
        let days: Vec<_> = hub.history(day(26), HistoryFilter::All, "").into_iter().map(|d| d.date).collect();
        assert_eq!(days, vec![day(24), day(20)]);
        let marks = hub.task_marks(day(1), day(30));
        assert_eq!(marks.len(), 4);
        assert!(marks.values().all(|unfinished| *unfinished));
        assert!(hub.task_marks(day(29), day(30)).is_empty());
    }

    #[test]
    fn history_can_be_filtered_and_searched() {
        let dir = tempfile::tempdir().unwrap();
        let mut hub = open(dir.path());
        hub.add_todo(day(20), "Read Rust book", 0).unwrap();
        hub.add_todo(day(20), "Gym", 0).unwrap();
        hub.add_todo(day(20), "rust exercises", 0).unwrap();
        hub.set_todo_completed(day(20), 0, true, 0).unwrap();
        hub.add_todo(day(24), "Groceries", 0).unwrap();
        hub.set_todo_completed(day(24), 0, true, 0).unwrap();

        let unfinished = hub.history(day(26), HistoryFilter::Unfinished, "");
        assert_eq!(unfinished.len(), 1, "day 24 has nothing unfinished");
        assert_eq!(unfinished[0].todos.iter().map(|(i, _)| *i).collect::<Vec<_>>(), vec![1, 2]);
        assert_eq!((unfinished[0].done, unfinished[0].total), (1, 3), "counts cover the whole day");

        let done = hub.history(day(26), HistoryFilter::Done, "");
        assert_eq!(done.iter().map(|d| d.date).collect::<Vec<_>>(), vec![day(24), day(20)]);

        let rust = hub.history(day(26), HistoryFilter::All, "  RUST ");
        assert_eq!(rust.len(), 1);
        let found: Vec<_> = rust[0].todos.iter().map(|(i, t)| (*i, t.text.as_str())).collect();
        assert_eq!(found, vec![(0, "Read Rust book"), (2, "rust exercises")], "ignores case, keeps positions");
        assert!(hub.history(day(26), HistoryFilter::Unfinished, "groceries").is_empty());
    }

    #[test]
    fn calendar_marks_show_whether_a_day_is_finished() {
        let dir = tempfile::tempdir().unwrap();
        let mut hub = open(dir.path());
        hub.add_todo(day(20), "Done", 0).unwrap();
        hub.set_todo_completed(day(20), 0, true, 0).unwrap();
        hub.add_todo(day(21), "Not yet", 0).unwrap();
        let marks = hub.task_marks(day(20), day(21));
        assert_eq!(marks, HashMap::from([(day(20), false), (day(21), true)]));
    }

    #[test]
    fn unfinished_tasks_move_to_today() {
        let dir = tempfile::tempdir().unwrap();
        let mut hub = open(dir.path());
        hub.add_todo(day(20), "Old unfinished", 0).unwrap();
        hub.add_todo(day(20), "Old done", 0).unwrap();
        hub.set_todo_completed(day(20), 1, true, 0).unwrap();
        hub.add_todo(day(24), "Newer unfinished", 0).unwrap();
        hub.add_todo(day(26), "Today's", 0).unwrap();
        hub.add_todo(day(28), "Future", 0).unwrap();
        assert_eq!(hub.unfinished_before(day(26)), 2);

        assert_eq!(hub.move_unfinished_to(day(26), 0).unwrap(), 2);
        assert_eq!(texts(&hub.todos(day(26))), vec!["Today's", "Old unfinished", "Newer unfinished"]);
        assert_eq!(texts(&hub.todos(day(20))), vec!["Old done"], "finished tasks stay");
        assert!(hub.todos(day(24)).is_empty(), "emptied days disappear");
        assert_eq!(texts(&hub.todos(day(28))), vec!["Future"], "later days aren't touched");
        assert_eq!(hub.unfinished_before(day(26)), 0);
        assert_eq!(hub.move_unfinished_to(day(26), 0).unwrap(), 0);
    }

    #[test]
    fn rewards_keep_unfinished_first_and_can_be_edited_and_restored() {
        let dir = tempfile::tempdir().unwrap();
        let mut hub = open(dir.path());
        hub.add_reward("Cake", 0).unwrap();
        hub.add_reward("Movie", 0).unwrap();
        hub.set_reward_completed(0, true, 0).unwrap(); // Cake done
        let names = |hub: &Hub| hub.rewards().iter().map(|r| r.name.clone()).collect::<Vec<_>>();
        assert_eq!(names(&hub), vec!["Movie", "Cake"]);
        hub.edit_reward(0, "Movie night", 0).unwrap();
        let removed = hub.delete_reward(1, 0).unwrap();
        assert_eq!(names(&hub), vec!["Movie night"]);
        hub.restore_reward(0, removed, 0).unwrap();
        assert_eq!(names(&hub), vec!["Movie night", "Cake"], "sorted again after restoring");
        assert!(hub.set_reward_completed(9, true, 0).is_err());
        assert!(hub.edit_reward(9, "x", 0).is_err());
    }

    #[test]
    fn timer_records_study_time_and_sessions() {
        let dir = tempfile::tempdir().unwrap();
        let mut hub = open(dir.path());
        hub.set_timer_settings(settings(10, 5, 1), 0, day(26)).unwrap();
        hub.toggle_timer(0, day(26)).unwrap();
        assert!(hub.tick(4 * SEC + 500, day(26)).unwrap().ended.is_empty());
        assert_eq!(hub.study_seconds_on(day(26)), 4);

        let ended = hub.tick(15 * SEC, day(26)).unwrap().ended;
        let modes: Vec<_> = ended.iter().map(|c| c.mode).collect();
        assert_eq!(modes, vec![TimerMode::Work, TimerMode::Break]);
        assert_eq!(hub.study_seconds_on(day(26)), 10);
        assert_eq!(hub.sessions_on(day(26)), 1);
        assert_eq!(hub.sessions_in_month_of(day(26)), 1);
        assert!(!hub.timer().is_running());
    }

    #[test]
    fn skipping_counts_the_time_spent_and_the_session() {
        let dir = tempfile::tempdir().unwrap();
        let mut hub = open(dir.path());
        hub.set_timer_settings(settings(100, 50, 1), 0, day(26)).unwrap();
        hub.toggle_timer(0, day(26)).unwrap();
        hub.skip_session(20 * SEC + 600, day(26)).unwrap(); // skip work after 20.6s
        assert_eq!(hub.study_seconds_on(day(26)), 21);
        assert_eq!(hub.timer().timer_mode, TimerMode::Break);
        let ended = hub.skip_session(21 * SEC, day(26)).unwrap(); // skip the break
        assert!(ended[0].all_done);
        assert_eq!(hub.sessions_on(day(26)), 1, "a skipped break still completes the session");
    }

    #[test]
    fn a_running_timer_survives_closing_the_app() {
        let dir = tempfile::tempdir().unwrap();
        {
            let mut hub = open(dir.path());
            hub.set_timer_settings(settings(60, 30, 1), 0, day(26)).unwrap();
            hub.toggle_timer(0, day(26)).unwrap();
            hub.tick(10 * SEC, day(26)).unwrap();
        } // app closed 10s into the work session

        // Reopened 70s later: work finished (50s more study), break is 20s in.
        let mut hub = Hub::open(dir.path(), 80 * SEC, day(26)).unwrap();
        assert!(hub.timer().is_running());
        assert_eq!(hub.timer().timer_mode, TimerMode::Break);
        assert_eq!(hub.study_seconds_on(day(26)), 60);
        let ended = hub.tick(80 * SEC, day(26)).unwrap().ended;
        assert_eq!(ended.len(), 1, "the work session that ended while closed is reported");
        assert!(hub.tick(81 * SEC, day(26)).unwrap().ended.is_empty(), "...only once");
    }

    #[test]
    fn changes_from_another_copy_of_the_app_are_picked_up() {
        let dir = tempfile::tempdir().unwrap();
        let mut main = open(dir.path());
        main.set_timer_settings(settings(100, 50, 1), 0, day(26)).unwrap();
        main.toggle_timer(0, day(26)).unwrap();
        main.add_todo(day(26), "From main", SEC).unwrap();

        // A notification button pauses the timer from a second copy of the app.
        std::thread::sleep(std::time::Duration::from_millis(20)); // a different file time
        let mut notification = Hub::open(dir.path(), 5 * SEC, day(26)).unwrap();
        notification.toggle_timer(5 * SEC, day(26)).unwrap();
        assert!(!notification.timer().is_running());

        // The main copy sees it on its next tick, and doesn't undo it.
        assert!(main.tick(6 * SEC, day(26)).unwrap().reloaded);
        assert!(!main.timer().is_running(), "the pause from the notification wins");
        assert!(!main.tick(6 * SEC + 250, day(26)).unwrap().reloaded, "only reloads once");
        main.add_todo(day(26), "Also from main", 7 * SEC).unwrap();
        let reopened = open(dir.path());
        assert!(!reopened.timer().is_running());
        assert_eq!(texts(&reopened.todos(day(26))), vec!["From main", "Also from main"]);
        assert_eq!(reopened.study_seconds_on(day(26)), 5);
    }

    #[test]
    fn hiding_the_app_does_not_undo_a_notification_button() {
        let dir = tempfile::tempdir().unwrap();
        let mut main = open(dir.path());
        main.set_timer_settings(settings(100, 50, 1), 0, day(26)).unwrap();
        main.toggle_timer(0, day(26)).unwrap();
        main.save_unless_changed(SEC).unwrap(); // the app is hidden

        std::thread::sleep(std::time::Duration::from_millis(20)); // a different file time
        let mut notification = Hub::open(dir.path(), 5 * SEC, day(26)).unwrap();
        notification.toggle_timer(5 * SEC, day(26)).unwrap(); // "Pause" in the notification

        // Coming back, the app passes through "hidden" again, which saves.
        assert!(main.save_unless_changed(9 * SEC).unwrap(), "it loads the newer file instead");
        assert!(!main.timer().is_running());
        assert!(!open(dir.path()).timer().is_running(), "the pause is still saved");
    }

    #[test]
    fn settings_are_clamped_and_saved() {
        let dir = tempfile::tempdir().unwrap();
        let mut hub = open(dir.path());
        let huge = TimerSettings {
            work_secs: 99_999,
            break_secs: 99_999,
            long_break_secs: 99_999,
            loops: 99,
            long_break_every: 99,
            auto_start: false,
        };
        hub.set_timer_settings(huge, 0, day(26)).unwrap();
        let reopened = open(dir.path());
        let s = reopened.timer_settings();
        assert_eq!(s.work_secs, MAX_WORK_SECS);
        assert_eq!(s.break_secs, MAX_BREAK_SECS);
        assert_eq!(s.long_break_secs, MAX_BREAK_SECS);
        assert_eq!(s.loops, MAX_LOOPS);
        assert_eq!(s.long_break_every, MAX_LOOPS);
        assert!(!s.auto_start);
        assert!(!reopened.timer().config.auto_start);

        hub.set_timer_settings(settings(10, 5, 0), 0, day(26)).unwrap();
        assert_eq!(hub.timer().config.total_loops, 1);
    }

    #[test]
    fn presets_apply_and_are_recognised() {
        let dir = tempfile::tempdir().unwrap();
        let mut hub = open(dir.path());
        assert_eq!(hub.timer_settings().preset_index(), Some(2), "the defaults are the desktop preset");
        hub.apply_preset(0, 0, day(26)).unwrap();
        let s = hub.timer_settings();
        assert_eq!((s.work_secs, s.break_secs, s.loops, s.long_break_every), (1500, 300, 4, 4));
        assert_eq!(s.preset_index(), Some(0));
        assert!(hub.apply_preset(7, 0, day(26)).is_err());
        hub.set_timer_settings(settings(10, 5, 1), 0, day(26)).unwrap();
        assert_eq!(hub.timer_settings().preset_index(), None);
    }

    #[test]
    fn reset_keeps_the_study_time_so_far() {
        let dir = tempfile::tempdir().unwrap();
        let mut hub = open(dir.path());
        hub.set_timer_settings(settings(100, 5, 1), 0, day(26)).unwrap();
        hub.toggle_timer(0, day(26)).unwrap();
        hub.reset_timer(7 * SEC + 600, day(26)).unwrap();
        assert_eq!(hub.study_seconds_on(day(26)), 8);
        assert!(hub.timer().is_at_start());
    }

    #[test]
    fn streaks_and_best_day() {
        let dir = tempfile::tempdir().unwrap();
        let mut hub = open(dir.path());
        let study = &mut hub.data.stats.daily_study_seconds;
        for (d, secs) in [(10, 600), (11, 60), (12, 1800), (20, 300), (21, 30), (24, 900), (25, 120)] {
            study.insert(day(d), secs);
        }
        // 21st has only 30s: below the 60s minimum, so it breaks the run.
        assert_eq!(hub.best_streak(), 3);
        assert_eq!(hub.current_streak(day(25)), 2, "24th and 25th");
        assert_eq!(hub.current_streak(day(26)), 2, "today not studied yet: streak still alive");
        assert_eq!(hub.current_streak(day(27)), 0, "a missed day ends it");
        assert_eq!(hub.best_day(), Some((day(12), 1800)));

        let series = hub.study_series(day(12), 4);
        assert_eq!(series, vec![(day(9), 0), (day(10), 600), (day(11), 60), (day(12), 1800)]);
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

    const DESKTOP_JSON: &str = r#"{
        "todos_by_date": {
            "2025-07-25": [ { "text": "A", "completed": false }, { "text": "B", "completed": true } ],
            "2025-07-26": []
        },
        "stats": { "daily_study_seconds": { "2025-07-25": 100 }, "daily_streaks": {}, "monthly_streaks": {} },
        "rewards": [ { "name": "Done", "completed": true }, { "name": "Open", "completed": false } ]
    }"#;

    #[test]
    fn import_replaces_data_but_keeps_settings() {
        let dir = tempfile::tempdir().unwrap();
        let mut hub = open(dir.path());
        hub.set_timer_settings(settings(25 * 60, 5 * 60, 4), 0, day(26)).unwrap();
        hub.add_todo(day(26), "Old task", 0).unwrap();
        let imported = hub.import_json(DESKTOP_JSON, 0).unwrap();
        assert_eq!((imported.days, imported.tasks, imported.rewards), (1, 2, 2));
        assert!(hub.todos(day(26)).is_empty());
        assert_eq!(hub.rewards()[0].name, "Open", "rewards are sorted after import");
        assert_eq!(hub.timer().config.total_loops, 4, "settings are kept");
        assert!(hub.import_json("not json", 0).is_err());
    }

    #[test]
    fn merge_combines_without_counting_twice() {
        let dir = tempfile::tempdir().unwrap();
        let mut hub = open(dir.path());
        let july25 = NaiveDate::from_ymd_opt(2025, 7, 25).unwrap();
        hub.add_todo(july25, "A", 0).unwrap(); // same text as in the file
        hub.add_todo(july25, "Phone only", 0).unwrap();
        hub.data.stats.daily_study_seconds.insert(july25, 40);
        hub.data.stats.daily_study_seconds.insert(day(26), 500);
        hub.add_reward("Open", 0).unwrap();
        hub.add_reward("Phone reward", 0).unwrap();

        let merged = hub.merge_json(DESKTOP_JSON, 0).unwrap();
        assert_eq!((merged.tasks_added, merged.study_days_updated, merged.rewards_added), (1, 1, 1));
        assert_eq!(texts(&hub.todos(july25)), vec!["A", "Phone only", "B"]);
        assert_eq!(hub.study_seconds_on(july25), 100, "the larger number wins");
        assert_eq!(hub.study_seconds_on(day(26)), 500, "our own days stay");
        let names: Vec<_> = hub.rewards().iter().map(|r| r.name.as_str()).collect();
        assert_eq!(names, vec!["Open", "Phone reward", "Done"]);

        // Merging the same file again changes nothing.
        let again = hub.merge_json(DESKTOP_JSON, 0).unwrap();
        assert_eq!((again.tasks_added, again.study_days_updated, again.rewards_added), (0, 0, 0));
        assert_eq!(hub.study_seconds_on(july25), 100);
        assert!(hub.merge_json("{", 0).is_err());
    }

    #[test]
    fn a_task_finished_in_either_copy_stays_finished() {
        let dir = tempfile::tempdir().unwrap();
        let mut hub = open(dir.path());
        let july25 = NaiveDate::from_ymd_opt(2025, 7, 25).unwrap();
        hub.add_todo(july25, "B", 0).unwrap(); // unfinished here, finished in the file
        hub.merge_json(DESKTOP_JSON, 0).unwrap();
        assert!(hub.todos(july25).iter().find(|t| t.text == "B").unwrap().completed);
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
    fn daily_backups_can_be_restored() {
        let dir = tempfile::tempdir().unwrap();
        {
            let mut hub = Hub::open(dir.path(), 0, day(25)).unwrap();
            hub.add_todo(day(25), "Before the mistake", 0).unwrap();
            hub.set_timer_settings(settings(1500, 300, 4), 0, day(25)).unwrap();
        }
        // The first open had no file to back up yet. Opening on the 26th backs up the data
        // as it was at the end of the 25th.
        let mut hub = Hub::open(dir.path(), 0, day(26)).unwrap();
        hub.delete_todo(day(25), 0, 0).unwrap();
        hub.set_timer_settings(settings(10, 5, 1), 0, day(26)).unwrap();
        assert_eq!(hub.backups(), vec![day(26)]);

        hub.restore_backup(day(26), 0).unwrap();
        assert_eq!(texts(&hub.todos(day(25))), vec!["Before the mistake"]);
        assert_eq!(hub.timer_settings().loops, 4, "settings come back too");
        assert!(hub.restore_backup(day(1), 0).is_err());
    }

    #[test]
    fn a_damaged_file_opens_with_a_warning() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join(super::super::persistence::FILE_NAME), "oops").unwrap();
        let hub = open(dir.path());
        assert!(hub.load_warning().is_some());
    }
}
