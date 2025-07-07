use std::time::{Duration, Instant};
use chrono::{Local, Datelike};
use rodio::{OutputStreamHandle, Sink, Source, source::SineWave};
use crate::app_data::Stats;

#[derive(PartialEq, Clone, Copy)]
pub enum TimerMode { Work, Break }

#[derive(PartialEq, Clone, Copy)]
pub enum TimerState { Paused, Running }

pub struct StudyTimer {
    pub work_duration: Duration,
    pub break_duration: Duration,
    pub total_loops: u32,
    pub timer_mode: TimerMode,
    pub timer_state: TimerState,
    pub time_remaining: Duration,
    pub current_loop: u32,
    pub stats: Stats,
    last_tick: Option<Instant>,
    pending_study_time: Duration,
}

impl StudyTimer {
    pub fn new(stats: Stats, work_duration: Duration, break_duration: Duration, total_loops: u32) -> Self {
        Self {
            work_duration,
            break_duration,
            total_loops,
            stats,
            timer_mode: TimerMode::Work,
            timer_state: TimerState::Paused,
            time_remaining: work_duration,
            current_loop: 1,
            last_tick: None,
            pending_study_time: Duration::ZERO,
        }
    }

    pub fn set_durations(&mut self, work_duration: Duration, break_duration: Duration, total_loops: u32) {
        self.work_duration = work_duration;
        self.break_duration = break_duration;
        self.total_loops = total_loops;
        self.reset();
    }

    pub fn tick(&mut self) -> bool {
        if self.timer_state != TimerState::Running { return false; }

        let now = Instant::now();
        let elapsed = self.last_tick.map_or(Duration::ZERO, |t| now.duration_since(t));
        self.last_tick = Some(now);

        if self.timer_mode == TimerMode::Work {
            self.pending_study_time += elapsed;
            if self.pending_study_time >= Duration::from_secs(1) {
                let whole_seconds = self.pending_study_time.as_secs();
                let today = Local::now().date_naive();
                *self.stats.daily_study_seconds.entry(today).or_insert(0) += whole_seconds;
                self.pending_study_time -= Duration::from_secs(whole_seconds);
            }
        }

        if self.time_remaining > elapsed {
            self.time_remaining -= elapsed;
            false
        } else {
            if self.timer_mode == TimerMode::Work {
                *self.stats.daily_study_seconds.entry(Local::now().date_naive()).or_insert(0) += self.time_remaining.as_secs();
            }
            self.time_remaining = Duration::ZERO;
            self.switch_session();
            true
        }
    }

    pub fn toggle_state(&mut self) {
        self.timer_state = match self.timer_state {
            TimerState::Paused => {
                self.last_tick = Some(Instant::now());
                TimerState::Running
            }
            TimerState::Running => {
                self.last_tick = None;
                TimerState::Paused
            }
        };
    }

    pub fn reset(&mut self) {
        self.timer_state = TimerState::Paused;
        self.timer_mode = TimerMode::Work;
        self.time_remaining = self.work_duration;
        self.current_loop = 1;
        self.last_tick = None;
    }

    fn switch_session(&mut self) {
        match self.timer_mode {
            TimerMode::Work => {
                self.timer_mode = TimerMode::Break;
                self.time_remaining = self.break_duration;
            }
            TimerMode::Break => {
                self.log_streak();
                if self.current_loop >= self.total_loops {
                    self.reset();
                    return;
                }
                self.current_loop += 1;
                self.timer_mode = TimerMode::Work;
                self.time_remaining = self.work_duration;
            }
        }
        self.last_tick = Some(Instant::now());
    }

    fn log_streak(&mut self) {
        let today = Local::now().date_naive();
        *self.stats.daily_streaks.entry(today).or_insert(0) += 1;
        let month_key = format!("{}-{}", today.year(), today.month());
        *self.stats.monthly_streaks.entry(month_key).or_insert(0) += 1;
    }

    pub fn get_session_switch_messages(&self) -> (&'static str, &'static str) {
        match self.timer_mode {
            TimerMode::Work => ("Work Complete!", "Time for a short break."),
            TimerMode::Break => ("Break Over!", "Time to get back to work."),
        }
    }
}

pub fn play_beep(stream_handle: &OutputStreamHandle) {
    if let Ok(sink) = Sink::try_new(stream_handle) {
        let source = SineWave::new(440.0).take_duration(Duration::from_millis(200)).amplify(0.20);
        sink.append(source);
        sink.detach();
    }
}