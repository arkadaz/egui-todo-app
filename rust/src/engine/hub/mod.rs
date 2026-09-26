//! All of Focus Hub's logic, with no UI code. The Flutter bridge (`crate::api`) is a thin
//! layer on top of this. Every method takes the current time as a parameter, so the tests
//! can control the clock.
//!
//! The data is kept in memory; after each change, only what it touched is written to the
//! database (see [`Change`]).
//!
//! Two copies of the app can be running at once: the normal app, and a short-lived copy
//! that Android starts when a notification button (Pause, Skip...) is tapped. Each copy has
//! its own `Hub`. Before every change, a `Hub` checks whether the other copy wrote to the
//! database and reloads if so, so neither overwrites the other's work.

use anyhow::{bail, Context, Result};
use chrono::{Duration as Days, NaiveDate};
use std::collections::{BTreeSet, HashMap};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use super::database::{self, Change, Database, Opened};
use super::dates::month_key;
use super::domain::{AppData, Reward, Settings, TodoItem};
use super::timer::{Completion, StudyTimer, TimerConfig, TimerMode};

mod background;
mod rewards;
mod stats;
mod tasks;
mod transfer;

#[cfg(test)]
mod tests;

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
    db: Database,
    data_dir: PathBuf,
    timer: StudyTimer,
    /// Study milliseconds not yet added to the stats (they're added in whole seconds).
    study_carry_ms: u64,
    last_save_ms: i64,
    /// What changed in memory and isn't in the database yet.
    unsaved: BTreeSet<Change>,
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
        let Opened { db, mut data, warning } = Database::open(data_dir, now_ms, today)?;
        clean_up(&mut data);
        let timer = timer_for(&data);
        let mut hub = Self {
            data,
            db,
            data_dir: data_dir.to_path_buf(),
            timer,
            study_carry_ms: 0,
            last_save_ms: now_ms,
            unsaved: BTreeSet::new(),
            pending_completions: Vec::new(),
            load_warning: warning,
        };
        let outcome = hub.timer.advance(now_ms);
        hub.pending_completions = hub.apply(outcome, today);
        hub.save(now_ms, [])?;
        Ok(hub)
    }

    pub fn load_warning(&self) -> Option<String> {
        self.load_warning.clone()
    }

    pub fn data_file(&self) -> String {
        self.db.path().display().to_string()
    }

    /// Writes `changes`, anything else not saved yet, and the timer, in one transaction.
    fn save(&mut self, now_ms: i64, changes: impl IntoIterator<Item = Change>) -> Result<()> {
        self.unsaved.extend(changes);
        self.data.timer = Some(self.timer.save());
        self.db.write(&self.data, &self.unsaved)?;
        self.unsaved.clear();
        self.last_save_ms = now_ms;
        Ok(())
    }

    /// Saves (when the app is hidden), unless the other copy of the app wrote to the
    /// database since this one last read or wrote it: then that version is loaded instead
    /// of being overwritten. Returns true if it loaded.
    pub fn save_unless_changed(&mut self, now_ms: i64) -> Result<bool> {
        if self.reload_if_changed()? {
            return Ok(true);
        }
        self.save(now_ms, [])?;
        Ok(false)
    }

    /// Reloads the data if another copy of the app wrote to the database. Returns true if
    /// it did. The other copy's version wins over anything not saved here yet.
    pub fn reload_if_changed(&mut self) -> Result<bool> {
        if !self.db.changed_elsewhere()? {
            return Ok(false);
        }
        let mut data = self.db.load()?;
        clean_up(&mut data);
        self.timer = timer_for(&data);
        self.data = data;
        self.study_carry_ms = 0;
        self.unsaved.clear();
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
        let reloaded = self.reload_if_changed()?;
        let outcome = self.timer.advance(now_ms);
        let mut ended = std::mem::take(&mut self.pending_completions);
        ended.extend(self.apply(outcome, today));
        let save_due = self.timer.is_running() && now_ms - self.last_save_ms >= SAVE_EVERY_MS;
        if !ended.is_empty() || save_due {
            self.save(now_ms, [])?;
        }
        Ok(Ticked { ended, reloaded })
    }

    /// Start or pause. Pausing counts the time up to now first.
    pub fn toggle_timer(&mut self, now_ms: i64, today: NaiveDate) -> Result<Vec<Completion>> {
        self.reload_if_changed()?;
        let outcome = self.timer.toggle(now_ms);
        let ended = self.apply(outcome, today);
        self.save(now_ms, [])?;
        Ok(ended)
    }

    /// Ends the current session now and moves to the next one.
    pub fn skip_session(&mut self, now_ms: i64, today: NaiveDate) -> Result<Vec<Completion>> {
        self.reload_if_changed()?;
        let outcome = self.timer.skip(now_ms);
        let ended = self.apply(outcome, today);
        self.flush_study_carry(today);
        self.save(now_ms, [])?;
        Ok(ended)
    }

    pub fn reset_timer(&mut self, now_ms: i64, today: NaiveDate) -> Result<()> {
        self.reload_if_changed()?;
        // Keep the study time done so far, then reset.
        let outcome = self.timer.advance(now_ms);
        self.apply(outcome, today);
        self.flush_study_carry(today);
        self.timer.reset();
        self.save(now_ms, [])
    }

    /// Changes the timer setup (clamped to the desktop app's limits). This resets the timer.
    pub fn set_timer_settings(&mut self, new: TimerSettings, now_ms: i64, today: NaiveDate) -> Result<()> {
        self.reload_if_changed()?;
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
        self.save(now_ms, [Change::Settings])
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
            self.unsaved.insert(Change::Stats(today));
        }
        for completion in &outcome.completed {
            // Like the desktop app, a session counts once its break is over.
            if completion.mode == TimerMode::Break {
                *self.data.stats.daily_streaks.entry(today).or_insert(0) += 1;
                *self.data.stats.monthly_streaks.entry(month_key(today)).or_insert(0) += 1;
                self.unsaved.insert(Change::Stats(today));
            }
        }
        outcome.completed
    }

    /// Rounds any leftover study milliseconds to the nearest second.
    fn flush_study_carry(&mut self, today: NaiveDate) {
        if self.study_carry_ms >= 500 {
            *self.data.stats.daily_study_seconds.entry(today).or_insert(0) += 1;
            self.unsaved.insert(Change::Stats(today));
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
        self.reload_if_changed()?;
        self.data.settings.gmt_offset_hours = hours;
        self.save(now_ms, [Change::Settings])
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
