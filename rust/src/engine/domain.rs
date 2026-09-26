//! The saved data. The JSON format matches the desktop (egui) Focus Hub, so a
//! `focushub_data.json` file can move between the two apps in either direction.

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::timer::TimerSave;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct TodoItem {
    pub text: String,
    pub completed: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq)]
pub struct Stats {
    #[serde(default)]
    pub daily_study_seconds: HashMap<NaiveDate, u64>,
    /// Sessions completed per day.
    #[serde(default)]
    pub daily_streaks: HashMap<NaiveDate, u32>,
    /// Sessions completed per month, keyed like "2026-9" (same keys as the desktop app).
    #[serde(default)]
    pub monthly_streaks: HashMap<String, u32>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Reward {
    pub name: String,
    pub completed: bool,
}

/// Timer and clock preferences. New in the mobile app: older files don't have this
/// section, so every field falls back to its default.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Settings {
    pub work_secs: u64,
    pub break_secs: u64,
    pub loops: u32,
    /// Hours east of GMT for the clock. `None` means "use the device's time zone".
    pub gmt_offset_hours: Option<i32>,
}

impl Default for Settings {
    /// The desktop app's defaults: 60 minutes of work, a 5 minute break, 1 loop.
    fn default() -> Self {
        Self {
            work_secs: 60 * 60,
            break_secs: 5 * 60,
            loops: 1,
            gmt_offset_hours: None,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq)]
pub struct AppData {
    pub todos_by_date: HashMap<NaiveDate, Vec<TodoItem>>,
    pub stats: Stats,
    pub rewards: Vec<Reward>,
    #[serde(default)]
    pub gif_path: Option<String>,
    /// New in the mobile app (the desktop app ignores it).
    #[serde(default)]
    pub settings: Settings,
    /// A running or paused timer, so it survives the app being closed.
    /// New in the mobile app (the desktop app ignores it).
    #[serde(default)]
    pub timer: Option<TimerSave>,
}
