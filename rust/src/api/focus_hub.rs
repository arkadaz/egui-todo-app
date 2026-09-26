//! The bridge to Flutter. flutter_rust_bridge turns everything `pub` in this file into Dart
//! (lib/src/rust/api/focus_hub.dart). It only translates: all the logic is in `crate::engine`.
//!
//! Dates cross the bridge as "YYYY-MM-DD" strings, and Rust does all the date math.

use anyhow::Result;
use chrono::{Datelike, FixedOffset, Local, NaiveDate, Utc};
use flutter_rust_bridge::frb;
use std::path::Path;

use crate::engine::dates;
use crate::engine::hub::Hub;
use crate::engine::timer::{Completion, TimerMode};

fn now_ms() -> i64 {
    Utc::now().timestamp_millis()
}

fn today() -> NaiveDate {
    Local::now().date_naive()
}

// ---- Data sent to Dart ------------------------------------------------------------------

pub struct TodoView {
    /// Position in that day's list (use it to change or delete the task).
    pub index: u32,
    pub text: String,
    pub completed: bool,
}

pub struct DayTodos {
    pub date: String,
    /// "Friday, September 25"
    pub title: String,
    pub todos: Vec<TodoView>,
}

pub struct CalendarDay {
    pub day: u32,
    pub date: String,
    pub is_today: bool,
    pub is_selected: bool,
    pub has_todos: bool,
}

pub struct CalendarMonth {
    /// "September 2026"
    pub title: String,
    pub year: i32,
    pub month: u32,
    /// Empty cells before day 1 (weeks start on Monday).
    pub leading_blanks: u32,
    pub days: Vec<CalendarDay>,
}

pub struct YearMonth {
    pub year: i32,
    pub month: u32,
}

pub struct RewardView {
    pub index: u32,
    pub name: String,
    pub completed: bool,
}

pub struct StatsView {
    /// "0d 3h 20m 5s"
    pub total_time: String,
    pub today_sessions: u32,
    /// "01:02:03"
    pub today_time: String,
    pub month_sessions: u32,
    /// "September"
    pub month_name: String,
}

pub struct TimerView {
    pub is_work: bool,
    pub is_running: bool,
    /// "Study Time" or "Break Time"
    pub mode_label: String,
    /// "(1/4)"
    pub loop_label: String,
    /// "59:59"
    pub remaining: String,
    /// 0.0 to 1.0
    pub progress: f64,
    pub work_secs: u32,
    pub break_secs: u32,
    pub loops: u32,
}

/// A session just ended.
pub struct SessionAlert {
    pub title: String,
    pub body: String,
}

/// A session end still to come, for scheduling a notification.
pub struct ScheduledAlert {
    pub delay_ms: u32,
    pub title: String,
    pub body: String,
}

pub struct TimeZoneView {
    pub follows_device: bool,
    /// The chosen offset (only meaningful when not following the device).
    pub offset_hours: i32,
    /// "Device (GMT+07:00)" or "GMT+05:00"
    pub label: String,
}

pub struct ImportSummary {
    pub days: u32,
    pub tasks: u32,
    pub rewards: u32,
}

fn alert(completion: &Completion) -> SessionAlert {
    let (title, body) = completion.message();
    SessionAlert { title: title.to_string(), body: body.to_string() }
}

// ---- Free functions (date math) -----------------------------------------------------------

/// Today's date on this device.
#[frb(sync)]
pub fn today_date() -> String {
    dates::iso(today())
}

/// "Saturday, September 26, 2026"
#[frb(sync)]
pub fn long_date(date: String) -> Result<String> {
    Ok(dates::long_date(dates::parse_date(&date)?))
}

#[frb(sync)]
pub fn month_of(date: String) -> Result<YearMonth> {
    let date = dates::parse_date(&date)?;
    Ok(YearMonth { year: date.year(), month: date.month() })
}

/// `delta` months after (or before, if negative) the given month.
#[frb(sync)]
pub fn shift_month(year: i32, month: u32, delta: i32) -> YearMonth {
    let (year, month) = dates::shift_month(year, month, delta);
    YearMonth { year, month }
}

// ---- The app's data and logic -------------------------------------------------------------

/// Dart gets a handle to this. The data stays in Rust.
#[frb(opaque)]
pub struct FocusHub {
    hub: Hub,
}

impl FocusHub {
    /// Opens the saved data in `data_dir` (the app's private folder).
    #[frb(sync)]
    pub fn open(data_dir: String) -> Result<FocusHub> {
        Ok(FocusHub { hub: Hub::open(Path::new(&data_dir), now_ms(), today())? })
    }

    /// A message to show once if the saved data was damaged, otherwise `null`.
    #[frb(sync)]
    pub fn load_warning(&self) -> Option<String> {
        self.hub.load_warning()
    }

    #[frb(sync)]
    pub fn data_file(&self) -> String {
        self.hub.data_file()
    }

    #[frb(sync)]
    pub fn save(&mut self) -> Result<()> {
        self.hub.save(now_ms())
    }

    // ---- Clock ----

    /// The time for the clock, "14:05:09", in the chosen time zone.
    #[frb(sync)]
    pub fn clock_text(&self) -> String {
        let format = "%H:%M:%S";
        match self.hub.gmt_offset_hours().and_then(|h| FixedOffset::east_opt(h * 3600)) {
            Some(offset) => Utc::now().with_timezone(&offset).format(format).to_string(),
            None => Local::now().format(format).to_string(),
        }
    }

    #[frb(sync)]
    pub fn time_zone(&self) -> TimeZoneView {
        match self.hub.gmt_offset_hours() {
            Some(hours) => TimeZoneView {
                follows_device: false,
                offset_hours: hours,
                label: dates::gmt_label(hours * 3600),
            },
            None => {
                let device = Local::now().offset().local_minus_utc();
                TimeZoneView {
                    follows_device: true,
                    offset_hours: device / 3600,
                    label: format!("Device ({})", dates::gmt_label(device)),
                }
            }
        }
    }

    /// `null` follows the device's time zone; otherwise hours from GMT (-12 to 14).
    #[frb(sync)]
    pub fn set_time_zone(&mut self, offset_hours: Option<i32>) -> Result<()> {
        self.hub.set_gmt_offset_hours(offset_hours, now_ms())
    }

    // ---- Timer ----

    #[frb(sync)]
    pub fn timer_view(&self) -> TimerView {
        let t = self.hub.timer();
        let is_work = t.timer_mode == TimerMode::Work;
        TimerView {
            is_work,
            is_running: t.is_running(),
            mode_label: if is_work { "Study Time" } else { "Break Time" }.to_string(),
            loop_label: format!("({}/{})", t.current_loop, t.total_loops),
            remaining: dates::mm_ss(t.time_remaining),
            progress: t.progress(),
            work_secs: t.work_duration.as_secs() as u32,
            break_secs: t.break_duration.as_secs() as u32,
            loops: t.total_loops,
        }
    }

    /// Call often (a few times a second). Returns the sessions that just ended.
    #[frb(sync)]
    pub fn tick(&mut self) -> Result<Vec<SessionAlert>> {
        Ok(self.hub.tick(now_ms(), today())?.iter().map(alert).collect())
    }

    /// Start or pause. Returns any sessions that ended right before pausing.
    #[frb(sync)]
    pub fn toggle_timer(&mut self) -> Result<Vec<SessionAlert>> {
        Ok(self.hub.toggle_timer(now_ms(), today())?.iter().map(alert).collect())
    }

    #[frb(sync)]
    pub fn reset_timer(&mut self) -> Result<()> {
        self.hub.reset_timer(now_ms(), today())
    }

    /// Changes the session lengths and loops (this resets the timer). Values are limited
    /// like the desktop app: work up to 120m 59s, break up to 60m 59s, 1 to 20 loops.
    #[frb(sync)]
    pub fn set_timer_settings(&mut self, work_secs: u32, break_secs: u32, loops: u32) -> Result<()> {
        self.hub
            .set_timer_settings(work_secs as u64, break_secs as u64, loops, now_ms(), today())
    }

    /// Every session end still to come, if the timer keeps running.
    #[frb(sync)]
    pub fn upcoming_alerts(&self) -> Vec<ScheduledAlert> {
        self.hub
            .upcoming(now_ms())
            .iter()
            .map(|(delay, completion)| {
                let (title, body) = completion.message();
                ScheduledAlert {
                    delay_ms: delay.as_millis().min(u32::MAX as u128) as u32,
                    title: title.to_string(),
                    body: body.to_string(),
                }
            })
            .collect()
    }

    // ---- To-dos ----

    #[frb(sync)]
    pub fn todos_for(&self, date: String) -> Result<Vec<TodoView>> {
        let date = dates::parse_date(&date)?;
        Ok(todo_views(self.hub.todos(date)))
    }

    #[frb(sync)]
    pub fn add_todo(&mut self, date: String, text: String) -> Result<()> {
        self.hub.add_todo(dates::parse_date(&date)?, &text, now_ms())
    }

    #[frb(sync)]
    pub fn set_todo_completed(&mut self, date: String, index: u32, completed: bool) -> Result<()> {
        self.hub
            .set_todo_completed(dates::parse_date(&date)?, index as usize, completed, now_ms())
    }

    #[frb(sync)]
    pub fn delete_todo(&mut self, date: String, index: u32) -> Result<()> {
        self.hub.delete_todo(dates::parse_date(&date)?, index as usize, now_ms())
    }

    /// Days before `date` with tasks, newest first.
    #[frb(sync)]
    pub fn history_before(&self, date: String) -> Result<Vec<DayTodos>> {
        let date = dates::parse_date(&date)?;
        Ok(self
            .hub
            .history_before(date)
            .into_iter()
            .map(|(day, todos)| DayTodos {
                date: dates::iso(day),
                title: dates::short_date(day),
                todos: todo_views(todos),
            })
            .collect())
    }

    #[frb(sync)]
    pub fn calendar_month(&self, year: i32, month: u32, selected_date: String) -> Result<CalendarMonth> {
        let first = dates::first_of_month(year, month)?;
        let selected = dates::parse_date(&selected_date).ok();
        let today = today();
        let with_todos = self.hub.days_with_todos(year, month);
        let days = (1..=dates::days_in_month(year, month)?)
            .filter_map(|day| first.with_day(day))
            .map(|date| {
                let day = date.day();
                CalendarDay {
                    day,
                    date: dates::iso(date),
                    is_today: date == today,
                    is_selected: Some(date) == selected,
                    has_todos: with_todos.contains(&day),
                }
            })
            .collect();
        Ok(CalendarMonth {
            title: dates::month_title(year, month)?,
            year,
            month,
            leading_blanks: first.weekday().num_days_from_monday(),
            days,
        })
    }

    // ---- Rewards ----

    #[frb(sync)]
    pub fn rewards(&self) -> Vec<RewardView> {
        self.hub
            .rewards()
            .iter()
            .enumerate()
            .map(|(i, r)| RewardView { index: i as u32, name: r.name.clone(), completed: r.completed })
            .collect()
    }

    #[frb(sync)]
    pub fn add_reward(&mut self, name: String) -> Result<()> {
        self.hub.add_reward(&name, now_ms())
    }

    #[frb(sync)]
    pub fn set_reward_completed(&mut self, index: u32, completed: bool) -> Result<()> {
        self.hub.set_reward_completed(index as usize, completed, now_ms())
    }

    #[frb(sync)]
    pub fn delete_reward(&mut self, index: u32) -> Result<()> {
        self.hub.delete_reward(index as usize, now_ms())
    }

    // ---- Stats ----

    #[frb(sync)]
    pub fn stats(&self) -> StatsView {
        let today = today();
        StatsView {
            total_time: dates::days_hours_minutes_seconds(self.hub.total_study_seconds()),
            today_sessions: self.hub.sessions_on(today),
            today_time: dates::hh_mm_ss(self.hub.study_seconds_on(today)),
            month_sessions: self.hub.sessions_in_month_of(today),
            month_name: today.format("%B").to_string(),
        }
    }

    // ---- Background ----

    /// The custom background image's path, or `null` for the built-in one.
    #[frb(sync)]
    pub fn background_path(&self) -> Option<String> {
        self.hub.background_path()
    }

    /// Saves a copy of a chosen image (GIF, PNG, JPG or WebP) as the background.
    #[frb(sync)]
    pub fn set_background(&mut self, file_name: String, bytes: Vec<u8>) -> Result<()> {
        self.hub.set_background(&file_name, &bytes, now_ms()).map(|_| ())
    }

    #[frb(sync)]
    pub fn clear_background(&mut self) -> Result<()> {
        self.hub.clear_background(now_ms())
    }

    // ---- Import / export ----

    /// All data as JSON (the desktop app's `focushub_data.json` format).
    #[frb(sync)]
    pub fn export_json(&mut self) -> Result<String> {
        self.hub.export_json(now_ms())
    }

    /// Replaces tasks, stats and rewards with those in a `focushub_data.json` file.
    #[frb(sync)]
    pub fn import_json(&mut self, json: String) -> Result<ImportSummary> {
        let imported = self.hub.import_json(&json, now_ms())?;
        Ok(ImportSummary { days: imported.days, tasks: imported.tasks, rewards: imported.rewards })
    }
}

fn todo_views(todos: Vec<crate::engine::domain::TodoItem>) -> Vec<TodoView> {
    todos
        .into_iter()
        .enumerate()
        .map(|(i, t)| TodoView { index: i as u32, text: t.text, completed: t.completed })
        .collect()
}
