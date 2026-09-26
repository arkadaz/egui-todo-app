//! The bridge to Flutter. `flutter_rust_bridge` turns everything `pub` in this file into Dart
//! (`lib/src/rust/api/focus_hub.dart`). It only translates: all the logic is in `crate::engine`.
//!
//! Dates cross the bridge as "YYYY-MM-DD" strings, and Rust does all the date math and
//! writes all the text the UI shows.

use anyhow::{Context, Result};
use chrono::{DateTime, Datelike, FixedOffset, Local, NaiveDate, TimeZone, Utc};
use flutter_rust_bridge::frb;
use std::fs;
use std::path::Path;

use crate::engine::dates;
use crate::engine::domain::{Reward, TodoItem};
use crate::engine::hub::{self, Hub, TimerSettings, PRESETS};
use crate::engine::images;
use crate::engine::timer::{Completion, StudyTimer, TimerMode};

fn now_ms() -> i64 {
    Utc::now().timestamp_millis()
}

fn today() -> NaiveDate {
    Local::now().date_naive()
}

// ---- Data sent to Dart ------------------------------------------------------------------

pub struct TodoView {
    /// Position in that day's list (use it to change, move or delete the task).
    pub index: u32,
    pub text: String,
    pub completed: bool,
}

pub struct DayTodos {
    pub date: String,
    /// "Friday, September 25"
    pub title: String,
    /// "2 of 6 done" or "All 6 done"
    pub summary: String,
    pub all_done: bool,
    /// The tasks that matched the filter and search.
    pub todos: Vec<TodoView>,
}

/// A month of the task history.
pub struct HistoryMonth {
    /// "2026-09": the same for as long as the month is in the history.
    pub key: String,
    /// "September 2026"
    pub title: String,
    /// "12 days · 5 unfinished" or "3 days · all done"
    pub summary: String,
    /// Newest first.
    pub days: Vec<DayTodos>,
}

/// Which of the history's tasks to show.
pub enum HistoryFilter {
    All,
    Unfinished,
    Done,
}

/// One Monday-to-Sunday week, for the folded calendar.
pub struct CalendarWeek {
    /// "Sep 21 – 27, 2026"
    pub title: String,
    pub days: Vec<CalendarDay>,
}

pub struct CalendarDay {
    pub day: u32,
    pub date: String,
    pub is_today: bool,
    pub is_selected: bool,
    pub has_todos: bool,
    /// Some of its tasks aren't done yet.
    pub has_unfinished: bool,
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
    /// Days in a row with at least a minute of study.
    pub streak_days: u32,
    pub best_streak_days: u32,
    /// "Tue, Sep 22 · 1h 20m", or `null` before any study.
    pub best_day: Option<String>,
    /// Study time in the last 7 days: "3h 5m"
    pub last_7_days_time: String,
}

/// One bar of the study chart.
pub struct ChartDay {
    pub date: String,
    /// "Mon" (7-day chart) or "26" (longer charts)
    pub label: String,
    pub minutes: f64,
    pub is_today: bool,
}

pub struct TimerView {
    pub is_work: bool,
    pub is_long_break: bool,
    pub is_running: bool,
    /// Nothing started yet (nothing to pause, skip or reset).
    pub is_at_start: bool,
    /// "Study Time", "Break Time" or "Long Break"
    pub mode_label: String,
    /// "(1/4)"
    pub loop_label: String,
    /// "59:59"
    pub remaining: String,
    /// 0.0 to 1.0
    pub progress: f64,
    pub work_secs: u32,
    pub break_secs: u32,
    pub long_break_secs: u32,
    pub loops: u32,
    /// 0 = no long breaks
    pub long_break_every: u32,
    pub auto_start: bool,
    /// The preset the settings match, if any.
    pub preset: Option<u32>,
}

pub struct PresetView {
    pub index: u32,
    pub name: String,
}

/// What happened since the last tick.
pub struct TickReport {
    /// Sessions that just ended.
    pub ended: Vec<SessionAlert>,
    /// The clock or the countdown now shows a different number, so the timer should be
    /// redrawn. (Ticks come 4 times a second; the numbers change about once a second.)
    pub redraw: bool,
    /// A notification button changed the data, so every screen should be redrawn.
    pub reloaded: bool,
    /// When to tick next: right after the next number on screen changes. Ticking only
    /// then (instead of on a fixed beat) keeps the phone asleep as much as possible.
    pub next_tick_ms: u32,
}

/// A session just ended.
pub struct SessionAlert {
    pub title: String,
    pub body: String,
}

/// What the ongoing "timer" notification should show.
pub struct TimerStatus {
    /// "Study Time · 1/4"
    pub title: String,
    /// "Ends at 14:35" or "Paused · 23:41 left"
    pub body: String,
    pub running: bool,
    /// When the current session ends (Unix milliseconds), for the live countdown.
    pub ends_at_ms: i64,
}

/// A session end still to come, for scheduling notifications ahead of time.
pub struct UpcomingEvent {
    pub delay_ms: u32,
    pub alert_title: String,
    pub alert_body: String,
    /// The ongoing notification right after it (`null` once every loop is done).
    pub next_status: Option<TimerStatus>,
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

pub struct MergeSummary {
    pub tasks_added: u32,
    pub study_days_updated: u32,
    pub rewards_added: u32,
}

/// A deleted task or reward, kept by the UI for "Undo".
pub struct Deleted {
    pub text: String,
    pub completed: bool,
}

pub struct BackupView {
    pub date: String,
    /// "Saturday, September 26, 2026"
    pub label: String,
}

fn alert(completion: Completion) -> SessionAlert {
    let (title, body) = completion.message();
    SessionAlert {
        title: title.to_string(),
        body: body.to_string(),
    }
}

fn mode_label(timer: &StudyTimer) -> &'static str {
    match (timer.timer_mode, timer.long_break) {
        (TimerMode::Work, _) => "Study Time",
        (TimerMode::Break, false) => "Break Time",
        (TimerMode::Break, true) => "Long Break",
    }
}

/// Runs once when Dart loads the library: Rust panics and logs show up in the app's log.
#[frb(init)]
pub fn init_app() {
    flutter_rust_bridge::setup_default_user_utils();
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
    Ok(YearMonth {
        year: date.year(),
        month: date.month(),
    })
}

/// `delta` months after (or before, if negative) the given month.
#[frb(sync)]
pub fn shift_month(year: i32, month: u32, delta: i32) -> YearMonth {
    let (year, month) = dates::shift_month(year, month, delta);
    YearMonth { year, month }
}

/// The date `days` days after (or before, if negative) `date`.
#[frb(sync)]
pub fn add_days(date: String, days: i32) -> Result<String> {
    let date = dates::parse_date(&date)?;
    let moved = date
        .checked_add_signed(chrono::Duration::days(days.into()))
        .ok_or_else(|| anyhow::anyhow!("That date is out of range."))?;
    Ok(dates::iso(moved))
}

/// The ready-made timer setups.
#[frb(sync)]
pub fn presets() -> Vec<PresetView> {
    PRESETS
        .iter()
        .enumerate()
        .map(|(i, p)| PresetView {
            index: i as u32,
            name: p.name.to_string(),
        })
        .collect()
}

// ---- The app's data and logic -------------------------------------------------------------

/// A chosen background, made ready for this screen by [`prepare_background`]. Dart only
/// holds a handle to it: the image itself stays in Rust.
#[frb(opaque)]
pub struct PreparedBackground {
    image: images::Prepared,
}

impl PreparedBackground {
    /// What to tell the user once it's set.
    #[frb(sync)]
    pub fn message(&self) -> String {
        let image = &self.image;
        if image.was_shrunk() {
            let (w, h) = image.original_size;
            let (new_w, new_h) = image.size;
            let frames = match image.frames {
                1 => String::new(),
                n => format!(", all {n} frames"),
            };
            format!("Background changed (resized from {w} × {h} to {new_w} × {new_h} for this screen{frames}).")
        } else {
            "Background changed.".to_string()
        }
    }
}

/// Reads the image at `path` (GIF, PNG, JPG or WebP) and shrinks it to fit a screen whose
/// longest side is `screen_side` pixels, keeping animations. Runs on a background thread,
/// since a big photo or a long GIF takes a moment; then pass it to `FocusHub::set_background`.
#[allow(clippy::needless_pass_by_value)] // Dart hands over an owned String
pub fn prepare_background(path: String, screen_side: u32) -> Result<PreparedBackground> {
    let bytes = fs::read(&path).context("Couldn't open that file.")?;
    Ok(PreparedBackground {
        image: images::prepare_background(&bytes, screen_side)?,
    })
}

/// Dart gets a handle to this. The data stays in Rust.
#[frb(opaque)]
pub struct FocusHub {
    hub: Hub,
    /// The wall-clock second and the countdown second at the last redraw.
    shown: Option<(i64, u64)>,
}

impl FocusHub {
    /// Opens the saved data in `data_dir` (the app's private folder).
    #[frb(sync)]
    pub fn open(data_dir: String) -> Result<FocusHub> {
        Ok(FocusHub {
            hub: Hub::open(Path::new(&data_dir), now_ms(), today())?,
            shown: None,
        })
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

    /// Saves (call it when the app is hidden). If a notification button changed the data
    /// meanwhile, loads that instead of overwriting it, and returns true.
    #[frb(sync)]
    pub fn save(&mut self) -> Result<bool> {
        self.hub.save_unless_changed(now_ms())
    }

    /// Picks up changes made by a notification button while the app was in the background.
    /// Returns true if anything changed.
    #[frb(sync)]
    pub fn reload_if_changed(&mut self) -> Result<bool> {
        self.hub.reload_if_changed()
    }

    // ---- Clock ----

    fn offset(&self) -> Option<FixedOffset> {
        self.hub
            .gmt_offset_hours()
            .and_then(|h| FixedOffset::east_opt(h * 3600))
    }

    /// Formats a moment with `format`, in the chosen time zone.
    fn format_time(&self, at: DateTime<Utc>, format: &str) -> String {
        match self.offset() {
            Some(offset) => at.with_timezone(&offset).format(format).to_string(),
            None => at.with_timezone(&Local).format(format).to_string(),
        }
    }

    /// The time for the clock, "14:05:09", in the chosen time zone.
    #[frb(sync)]
    pub fn clock_text(&self) -> String {
        self.format_time(Utc::now(), "%H:%M:%S")
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
        let s = self.hub.timer_settings();
        TimerView {
            is_work: t.timer_mode == TimerMode::Work,
            is_long_break: t.timer_mode == TimerMode::Break && t.long_break,
            is_running: t.is_running(),
            is_at_start: t.is_at_start(),
            mode_label: mode_label(t).to_string(),
            loop_label: format!("({}/{})", t.current_loop, t.config.total_loops),
            remaining: dates::mm_ss(t.time_remaining),
            progress: t.progress(),
            work_secs: s.work_secs as u32,
            break_secs: s.break_secs as u32,
            long_break_secs: s.long_break_secs as u32,
            loops: s.loops,
            long_break_every: s.long_break_every,
            auto_start: s.auto_start,
            preset: s.preset_index().map(|i| i as u32),
        }
    }

    /// Call often (a few times a second).
    #[frb(sync)]
    pub fn tick(&mut self) -> Result<TickReport> {
        let now = now_ms();
        let ticked = self.hub.tick(now, today())?;
        let shown = Some((now.div_euclid(1000), self.hub.timer().time_remaining.as_secs()));
        let redraw = shown != self.shown || !ticked.ended.is_empty() || ticked.reloaded;
        self.shown = shown;
        Ok(TickReport {
            ended: ticked.ended.iter().copied().map(alert).collect(),
            redraw,
            reloaded: ticked.reloaded,
            next_tick_ms: next_change_ms(now, self.hub.timer()),
        })
    }

    /// Start or pause. Returns any sessions that ended right before pausing.
    #[frb(sync)]
    pub fn toggle_timer(&mut self) -> Result<Vec<SessionAlert>> {
        Ok(self
            .hub
            .toggle_timer(now_ms(), today())?
            .iter()
            .copied()
            .map(alert)
            .collect())
    }

    /// Ends the current session now and moves on to the next one.
    #[frb(sync)]
    pub fn skip_session(&mut self) -> Result<Vec<SessionAlert>> {
        Ok(self
            .hub
            .skip_session(now_ms(), today())?
            .iter()
            .copied()
            .map(alert)
            .collect())
    }

    #[frb(sync)]
    pub fn reset_timer(&mut self) -> Result<()> {
        self.hub.reset_timer(now_ms(), today())
    }

    /// Changes the timer setup (this resets the timer). Values are limited like the desktop
    /// app: work up to 120m 59s, breaks up to 60m 59s, 1 to 20 loops.
    #[frb(sync)]
    pub fn set_timer_settings(
        &mut self,
        work_secs: u32,
        break_secs: u32,
        long_break_secs: u32,
        loops: u32,
        long_break_every: u32,
        auto_start: bool,
    ) -> Result<()> {
        let settings = TimerSettings {
            work_secs: u64::from(work_secs),
            break_secs: u64::from(break_secs),
            long_break_secs: u64::from(long_break_secs),
            loops,
            long_break_every,
            auto_start,
        };
        self.hub.set_timer_settings(settings, now_ms(), today())
    }

    #[frb(sync)]
    pub fn apply_preset(&mut self, index: u32) -> Result<()> {
        self.hub.apply_preset(index as usize, now_ms(), today())
    }

    /// What the ongoing notification should show now (`null` if the timer hasn't started).
    #[frb(sync)]
    pub fn status_now(&self) -> Option<TimerStatus> {
        let timer = self.hub.timer();
        (!timer.is_at_start()).then(|| self.status_of(timer, now_ms()))
    }

    fn status_of(&self, timer: &StudyTimer, at_ms: i64) -> TimerStatus {
        let ends_at_ms = at_ms + timer.time_remaining.as_millis() as i64;
        let body = if timer.is_running() {
            let ends = Utc.timestamp_millis_opt(ends_at_ms).single().unwrap_or_else(Utc::now);
            format!("Ends at {}", self.format_time(ends, "%H:%M"))
        } else {
            format!("Paused · {} left", dates::mm_ss(timer.time_remaining))
        };
        TimerStatus {
            title: format!(
                "{} · {}/{}",
                mode_label(timer),
                timer.current_loop,
                timer.config.total_loops
            ),
            body,
            running: timer.is_running(),
            ends_at_ms,
        }
    }

    /// Every session end still to come if the timer keeps running.
    #[frb(sync)]
    pub fn upcoming_events(&self) -> Vec<UpcomingEvent> {
        let now = now_ms();
        self.hub
            .upcoming(now)
            .iter()
            .map(|(delay, completion, after)| {
                let (title, body) = completion.message();
                let delay_ms = delay.as_millis().min(u128::from(u32::MAX)) as u32;
                UpcomingEvent {
                    delay_ms,
                    alert_title: title.to_string(),
                    alert_body: body.to_string(),
                    next_status: (!completion.all_done).then(|| self.status_of(after, now + i64::from(delay_ms))),
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
    pub fn edit_todo(&mut self, date: String, index: u32, text: String) -> Result<()> {
        self.hub
            .edit_todo(dates::parse_date(&date)?, index as usize, &text, now_ms())
    }

    /// Deletes a task. Returns it, so "Undo" can put it back with `restore_todo`.
    #[frb(sync)]
    pub fn delete_todo(&mut self, date: String, index: u32) -> Result<Deleted> {
        let removed = self
            .hub
            .delete_todo(dates::parse_date(&date)?, index as usize, now_ms())?;
        Ok(Deleted {
            text: removed.text,
            completed: removed.completed,
        })
    }

    #[frb(sync)]
    pub fn restore_todo(&mut self, date: String, index: u32, text: String, completed: bool) -> Result<()> {
        let todo = TodoItem { text, completed };
        self.hub
            .restore_todo(dates::parse_date(&date)?, index as usize, todo, now_ms())
    }

    /// Moves a task to position `to` within its day.
    #[frb(sync)]
    pub fn move_todo(&mut self, date: String, from: u32, to: u32) -> Result<()> {
        self.hub
            .move_todo(dates::parse_date(&date)?, from as usize, to as usize, now_ms())
    }

    /// Days before `date` with tasks that pass `filter` and contain `search` (ignoring
    /// case), grouped by month, newest first.
    #[frb(sync)]
    pub fn history(&self, before: String, filter: HistoryFilter, search: String) -> Result<Vec<HistoryMonth>> {
        let filter = match filter {
            HistoryFilter::All => hub::HistoryFilter::All,
            HistoryFilter::Unfinished => hub::HistoryFilter::Unfinished,
            HistoryFilter::Done => hub::HistoryFilter::Done,
        };
        let mut months: Vec<HistoryMonth> = Vec::new();
        let mut unfinished_in_month = 0;
        for day in self.hub.history(dates::parse_date(&before)?, filter, &search) {
            let key = day.date.format("%Y-%m").to_string();
            if months.last().is_none_or(|month| month.key != key) {
                if let Some(month) = months.last_mut() {
                    month.summary = month_summary(month.days.len(), unfinished_in_month);
                }
                unfinished_in_month = 0;
                months.push(HistoryMonth {
                    key,
                    title: day.date.format("%B %Y").to_string(),
                    summary: String::new(),
                    days: Vec::new(),
                });
            }
            unfinished_in_month += day.total - day.done;
            let month = months.last_mut().expect("just pushed");
            month.days.push(DayTodos {
                date: dates::iso(day.date),
                title: dates::short_date(day.date),
                summary: if day.done == day.total {
                    format!("All {} done", day.total)
                } else {
                    format!("{} of {} done", day.done, day.total)
                },
                all_done: day.done == day.total,
                todos: day
                    .todos
                    .into_iter()
                    .map(|(index, todo)| TodoView {
                        index: index as u32,
                        text: todo.text,
                        completed: todo.completed,
                    })
                    .collect(),
            });
        }
        if let Some(month) = months.last_mut() {
            month.summary = month_summary(month.days.len(), unfinished_in_month);
        }
        Ok(months)
    }

    /// How many unfinished tasks there are on days before `date`.
    #[frb(sync)]
    pub fn unfinished_before(&self, date: String) -> Result<u32> {
        Ok(self.hub.unfinished_before(dates::parse_date(&date)?))
    }

    /// Moves every unfinished task from earlier days onto `date`. Returns how many moved.
    #[frb(sync)]
    pub fn move_unfinished_to(&mut self, date: String) -> Result<u32> {
        self.hub.move_unfinished_to(dates::parse_date(&date)?, now_ms())
    }

    #[frb(sync)]
    pub fn calendar_month(&self, year: i32, month: u32, selected_date: String) -> Result<CalendarMonth> {
        let first = dates::first_of_month(year, month)?;
        let selected = dates::parse_date(&selected_date).ok();
        let last = first.with_day(dates::days_in_month(year, month)?).unwrap_or(first);
        let days = self.calendar_days(first, last, selected);
        Ok(CalendarMonth {
            title: dates::month_title(year, month)?,
            year,
            month,
            leading_blanks: first.weekday().num_days_from_monday(),
            days,
        })
    }

    /// The Monday-to-Sunday week around `selected_date`.
    #[frb(sync)]
    pub fn calendar_week(&self, selected_date: String) -> Result<CalendarWeek> {
        let selected = dates::parse_date(&selected_date)?;
        let monday = selected - chrono::Days::new(selected.weekday().num_days_from_monday().into());
        let sunday = monday + chrono::Days::new(6);
        Ok(CalendarWeek {
            title: dates::week_title(monday),
            days: self.calendar_days(monday, sunday, Some(selected)),
        })
    }

    fn calendar_days(&self, first: NaiveDate, last: NaiveDate, selected: Option<NaiveDate>) -> Vec<CalendarDay> {
        let today = today();
        let marks = self.hub.task_marks(first, last);
        first
            .iter_days()
            .take_while(|date| *date <= last)
            .map(|date| CalendarDay {
                day: date.day(),
                date: dates::iso(date),
                is_today: date == today,
                is_selected: Some(date) == selected,
                has_todos: marks.contains_key(&date),
                has_unfinished: marks.get(&date).copied().unwrap_or(false),
            })
            .collect()
    }

    // ---- Rewards ----

    #[frb(sync)]
    pub fn rewards(&self) -> Vec<RewardView> {
        self.hub
            .rewards()
            .iter()
            .enumerate()
            .map(|(i, r)| RewardView {
                index: i as u32,
                name: r.name.clone(),
                completed: r.completed,
            })
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
    pub fn edit_reward(&mut self, index: u32, name: String) -> Result<()> {
        self.hub.edit_reward(index as usize, &name, now_ms())
    }

    /// Deletes a reward. Returns it, so "Undo" can put it back with `restore_reward`.
    #[frb(sync)]
    pub fn delete_reward(&mut self, index: u32) -> Result<Deleted> {
        let removed = self.hub.delete_reward(index as usize, now_ms())?;
        Ok(Deleted {
            text: removed.name,
            completed: removed.completed,
        })
    }

    #[frb(sync)]
    pub fn restore_reward(&mut self, index: u32, name: String, completed: bool) -> Result<()> {
        self.hub
            .restore_reward(index as usize, Reward { name, completed }, now_ms())
    }

    // ---- Stats ----

    #[frb(sync)]
    pub fn stats(&self) -> StatsView {
        let today = today();
        let last_7: u64 = self.hub.study_series(today, 7).iter().map(|(_, secs)| secs).sum();
        StatsView {
            total_time: dates::days_hours_minutes_seconds(self.hub.total_study_seconds()),
            today_sessions: self.hub.sessions_on(today),
            today_time: dates::hh_mm_ss(self.hub.study_seconds_on(today)),
            month_sessions: self.hub.sessions_in_month_of(today),
            month_name: today.format("%B").to_string(),
            streak_days: self.hub.current_streak(today),
            best_streak_days: self.hub.best_streak(),
            best_day: self
                .hub
                .best_day()
                .map(|(day, secs)| format!("{} · {}", day.format("%a, %b %-d"), dates::hours_minutes(secs))),
            last_7_days_time: dates::hours_minutes(last_7),
        }
    }

    /// Study minutes for each of the last `days` days, oldest first.
    #[frb(sync)]
    pub fn study_chart(&self, days: u32) -> Vec<ChartDay> {
        let today = today();
        self.hub
            .study_series(today, days.clamp(1, 366))
            .into_iter()
            .map(|(day, secs)| ChartDay {
                date: dates::iso(day),
                label: if days <= 7 {
                    day.format("%a").to_string()
                } else {
                    day.format("%-d").to_string()
                },
                minutes: secs as f64 / 60.0,
                is_today: day == today,
            })
            .collect()
    }

    // ---- Background ----

    /// Whether the background GIF plays (off saves battery).
    #[frb(sync)]
    pub fn animate_background(&self) -> bool {
        self.hub.animate_background()
    }

    #[frb(sync)]
    pub fn set_animate_background(&mut self, animate: bool) -> Result<()> {
        self.hub.set_animate_background(animate, now_ms())
    }

    /// Whether the app already offered to put its icon on the Home screen (once is enough).
    #[frb(sync)]
    pub fn home_icon_offered(&self) -> bool {
        self.hub.home_icon_offered()
    }

    #[frb(sync)]
    pub fn set_home_icon_offered(&mut self) -> Result<()> {
        self.hub.set_home_icon_offered(now_ms())
    }

    /// The custom background image's path, or `null` for the built-in one.
    #[frb(sync)]
    pub fn background_path(&self) -> Option<String> {
        self.hub.background_path()
    }

    /// Saves an image from [`prepare_background`] as the background.
    #[frb(sync)]
    pub fn set_background(&mut self, image: PreparedBackground) -> Result<()> {
        let image = image.image;
        let file_name = format!("background.{}", image.extension);
        self.hub.set_background(&file_name, &image.bytes, now_ms()).map(|_| ())
    }

    #[frb(sync)]
    pub fn clear_background(&mut self) -> Result<()> {
        self.hub.clear_background(now_ms())
    }

    // ---- Import / export / backups ----

    /// All data as JSON (the desktop app's `focushub_data.json` format).
    #[frb(sync)]
    pub fn export_json(&mut self) -> Result<String> {
        self.hub.export_json()
    }

    /// Replaces tasks, stats and rewards with those in a `focushub_data.json` file.
    #[frb(sync)]
    pub fn import_json(&mut self, json: String) -> Result<ImportSummary> {
        let imported = self.hub.import_json(&json, now_ms())?;
        Ok(ImportSummary {
            days: imported.days,
            tasks: imported.tasks,
            rewards: imported.rewards,
        })
    }

    /// Adds the tasks, stats and rewards from a `focushub_data.json` file to what's here,
    /// without counting anything twice.
    #[frb(sync)]
    pub fn merge_json(&mut self, json: String) -> Result<MergeSummary> {
        let merged = self.hub.merge_json(&json, now_ms())?;
        Ok(MergeSummary {
            tasks_added: merged.tasks_added,
            study_days_updated: merged.study_days_updated,
            rewards_added: merged.rewards_added,
        })
    }

    /// The daily backups, newest first.
    #[frb(sync)]
    pub fn backups(&self) -> Vec<BackupView> {
        self.hub
            .backups()
            .into_iter()
            .map(|day| BackupView {
                date: dates::iso(day),
                label: dates::long_date(day),
            })
            .collect()
    }

    /// Replaces everything with a daily backup.
    #[frb(sync)]
    pub fn restore_backup(&mut self, date: String) -> Result<()> {
        self.hub.restore_backup(dates::parse_date(&date)?, now_ms())
    }
}

/// "12 days · 5 unfinished", "1 day · all done"
fn month_summary(days: usize, unfinished: usize) -> String {
    let days = if days == 1 {
        "1 day".to_string()
    } else {
        format!("{days} days")
    };
    match unfinished {
        0 => format!("{days} · all done"),
        n => format!("{days} · {n} unfinished"),
    }
}

/// Milliseconds until the clock or the countdown next shows a different number, plus a
/// little, so the tick lands just after the change.
fn next_change_ms(now_ms: i64, timer: &StudyTimer) -> u32 {
    const MARGIN_MS: i64 = 5;
    let clock = 1000 - now_ms.rem_euclid(1000);
    let countdown = if timer.is_running() {
        // "59:59" shows whole seconds left, rounded down: it changes when the milliseconds
        // run out.
        timer.time_remaining.as_millis() as i64 % 1000 + 1
    } else {
        1000
    };
    (clock.min(countdown) + MARGIN_MS) as u32
}

fn todo_views(todos: Vec<TodoItem>) -> Vec<TodoView> {
    todos
        .into_iter()
        .enumerate()
        .map(|(i, t)| TodoView {
            index: i as u32,
            text: t.text,
            completed: t.completed,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::timer::TimerConfig;
    use std::time::Duration;

    #[test]
    fn ticks_land_just_after_the_next_change() {
        let config = TimerConfig {
            work: Duration::from_secs(60),
            short_break: Duration::from_secs(10),
            long_break: Duration::from_secs(20),
            total_loops: 1,
            long_break_every: 0,
            auto_start: true,
        };
        let mut timer = StudyTimer::new(config);
        // Stopped: only the clock changes, on the next whole second.
        assert_eq!(next_change_ms(10_200, &timer), 805);
        // Running since 10.2 s: at 10.5 s, 59.7 s are left, so "00:59" turns into "00:58"
        // 0.7 s later, but the clock changes first (at 11 s).
        timer.toggle(10_200);
        timer.advance(10_500);
        assert_eq!(next_change_ms(10_500, &timer), 505);
        // At 10.95 s the countdown (59.25 s left) changes before the clock does.
        timer.advance(10_950);
        assert_eq!(next_change_ms(10_950, &timer), 55);
    }

    #[test]
    fn months_are_summed_up() {
        assert_eq!(month_summary(12, 5), "12 days · 5 unfinished");
        assert_eq!(month_summary(1, 0), "1 day · all done");
    }
}
