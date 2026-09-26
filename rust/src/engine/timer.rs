//! The Pomodoro timer: work sessions and breaks, repeated for a number of loops.
//!
//! The desktop version measured time with `Instant`. On a phone that stops counting while
//! the device sleeps, so this version uses wall-clock milliseconds passed in by the caller.
//! That also makes it easy to test, and lets a running timer be saved and restored.

use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(PartialEq, Eq, Clone, Copy, Debug, Serialize, Deserialize)]
pub enum TimerMode {
    Work,
    Break,
}

#[derive(PartialEq, Eq, Clone, Copy, Debug)]
pub enum TimerState {
    Paused,
    Running,
}

/// A session that just ended.
#[derive(PartialEq, Eq, Clone, Copy, Debug)]
pub struct Completion {
    /// The kind of session that ended.
    pub mode: TimerMode,
    /// True when this was the last break, so every loop is finished.
    pub all_done: bool,
}

impl Completion {
    /// The alert shown when this session ends.
    pub fn message(&self) -> (&'static str, &'static str) {
        match (self.mode, self.all_done) {
            (TimerMode::Work, _) => ("Work Complete!", "Time for a short break."),
            (TimerMode::Break, false) => ("Break Over!", "Time to get back to work."),
            (TimerMode::Break, true) => ("All Sessions Done!", "Great work! Every loop is complete."),
        }
    }
}

/// What happened during one `advance`.
#[derive(Default, Debug, PartialEq)]
pub struct TickOutcome {
    /// Time spent in work sessions, for the study statistics.
    pub study: Duration,
    /// Sessions that ended, oldest first.
    pub completed: Vec<Completion>,
}

/// The timer's state as saved to disk.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct TimerSave {
    pub mode: TimerMode,
    pub running: bool,
    pub remaining_ms: u64,
    pub current_loop: u32,
    /// When a running timer was last brought up to date (Unix milliseconds).
    pub last_update_ms: Option<i64>,
}

#[derive(Clone, Debug)]
pub struct StudyTimer {
    pub work_duration: Duration,
    pub break_duration: Duration,
    pub total_loops: u32,
    pub timer_mode: TimerMode,
    pub timer_state: TimerState,
    pub time_remaining: Duration,
    pub current_loop: u32,
    last_update_ms: Option<i64>,
}

impl StudyTimer {
    pub fn new(work_duration: Duration, break_duration: Duration, total_loops: u32) -> Self {
        Self {
            work_duration,
            break_duration,
            total_loops: total_loops.max(1),
            timer_mode: TimerMode::Work,
            timer_state: TimerState::Paused,
            time_remaining: work_duration,
            current_loop: 1,
            last_update_ms: None,
        }
    }

    /// Changes the session lengths and loop count, then resets (like the desktop app).
    pub fn set_durations(&mut self, work_duration: Duration, break_duration: Duration, total_loops: u32) {
        self.work_duration = work_duration;
        self.break_duration = break_duration;
        self.total_loops = total_loops.max(1);
        self.reset();
    }

    pub fn is_running(&self) -> bool {
        self.timer_state == TimerState::Running
    }

    /// Starts a paused timer, or pauses a running one (counting the time up to `now_ms`
    /// first, which may finish sessions).
    pub fn toggle(&mut self, now_ms: i64) -> TickOutcome {
        match self.timer_state {
            TimerState::Paused => {
                self.timer_state = TimerState::Running;
                self.last_update_ms = Some(now_ms);
                TickOutcome::default()
            }
            TimerState::Running => {
                let outcome = self.advance(now_ms);
                if self.is_running() {
                    self.timer_state = TimerState::Paused;
                    self.last_update_ms = None;
                }
                outcome
            }
        }
    }

    pub fn reset(&mut self) {
        self.timer_state = TimerState::Paused;
        self.timer_mode = TimerMode::Work;
        self.time_remaining = self.work_duration;
        self.current_loop = 1;
        self.last_update_ms = None;
    }

    /// Brings a running timer up to `now_ms`. Handles any amount of elapsed time: if the
    /// app was asleep through several sessions, they all complete, in order.
    pub fn advance(&mut self, now_ms: i64) -> TickOutcome {
        let mut outcome = TickOutcome::default();
        if !self.is_running() {
            return outcome;
        }
        let last = self.last_update_ms.unwrap_or(now_ms);
        // A clock that moved backwards counts as no time passing.
        let mut elapsed = Duration::from_millis(now_ms.saturating_sub(last).max(0) as u64);
        self.last_update_ms = Some(now_ms);

        // Every pass either uses up `elapsed` or finishes a session. At most
        // 2 * total_loops sessions can finish before the timer resets, so this ends.
        loop {
            if elapsed < self.time_remaining {
                if self.timer_mode == TimerMode::Work {
                    outcome.study += elapsed;
                }
                self.time_remaining -= elapsed;
                break;
            }
            if self.timer_mode == TimerMode::Work {
                outcome.study += self.time_remaining;
            }
            elapsed -= self.time_remaining;
            let mode = self.timer_mode;
            let all_done = self.switch_session();
            outcome.completed.push(Completion { mode, all_done });
            if all_done {
                break;
            }
        }
        outcome
    }

    /// Moves to the next session. Returns true if every loop is done (the timer resets).
    fn switch_session(&mut self) -> bool {
        match self.timer_mode {
            TimerMode::Work => {
                self.timer_mode = TimerMode::Break;
                self.time_remaining = self.break_duration;
                false
            }
            TimerMode::Break => {
                if self.current_loop >= self.total_loops {
                    self.reset();
                    return true;
                }
                self.current_loop += 1;
                self.timer_mode = TimerMode::Work;
                self.time_remaining = self.work_duration;
                false
            }
        }
    }

    /// Every session end still to come if the timer keeps running, with how long from
    /// `now_ms` until each one. Used to schedule notifications ahead of time.
    pub fn upcoming(&self, now_ms: i64) -> Vec<(Duration, Completion)> {
        let mut list = Vec::new();
        if !self.is_running() {
            return list;
        }
        let since_update = self
            .last_update_ms
            .map(|last| Duration::from_millis(now_ms.saturating_sub(last).max(0) as u64))
            .unwrap_or(Duration::ZERO);
        let mut sim = self.clone();
        let mut at = Duration::ZERO;
        loop {
            at += sim.time_remaining;
            let mode = sim.timer_mode;
            let all_done = sim.switch_session();
            list.push((at.saturating_sub(since_update), Completion { mode, all_done }));
            if all_done {
                break;
            }
        }
        list
    }

    /// The length of the current session.
    pub fn session_duration(&self) -> Duration {
        match self.timer_mode {
            TimerMode::Work => self.work_duration,
            TimerMode::Break => self.break_duration,
        }
    }

    /// How much of the current session is done, from 0.0 to 1.0.
    pub fn progress(&self) -> f64 {
        let total = self.session_duration().as_secs_f64();
        if total <= 0.0 {
            return 0.0;
        }
        (1.0 - self.time_remaining.as_secs_f64() / total).clamp(0.0, 1.0)
    }

    pub fn save(&self) -> TimerSave {
        TimerSave {
            mode: self.timer_mode,
            running: self.is_running(),
            remaining_ms: self.time_remaining.as_millis() as u64,
            current_loop: self.current_loop,
            last_update_ms: self.last_update_ms,
        }
    }

    /// Restores a saved state, fixing anything that doesn't fit the current settings.
    pub fn restore(&mut self, saved: &TimerSave) {
        self.timer_mode = saved.mode;
        self.current_loop = saved.current_loop.clamp(1, self.total_loops);
        self.time_remaining = Duration::from_millis(saved.remaining_ms).min(self.session_duration());
        match (saved.running, saved.last_update_ms) {
            (true, Some(last)) => {
                self.timer_state = TimerState::Running;
                self.last_update_ms = Some(last);
            }
            _ => {
                self.timer_state = TimerState::Paused;
                self.last_update_ms = None;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SEC: i64 = 1000;

    fn timer(work: u64, brk: u64, loops: u32) -> StudyTimer {
        StudyTimer::new(Duration::from_secs(work), Duration::from_secs(brk), loops)
    }

    #[test]
    fn starts_paused_on_work() {
        let t = timer(1500, 300, 4);
        assert_eq!(t.timer_mode, TimerMode::Work);
        assert_eq!(t.timer_state, TimerState::Paused);
        assert_eq!(t.time_remaining, Duration::from_secs(1500));
        assert_eq!(t.current_loop, 1);
    }

    #[test]
    fn paused_timer_does_not_move() {
        let mut t = timer(10, 5, 1);
        assert_eq!(t.advance(50 * SEC), TickOutcome::default());
        assert_eq!(t.time_remaining, Duration::from_secs(10));
    }

    #[test]
    fn counts_down_and_records_study_time() {
        let mut t = timer(10, 5, 1);
        t.toggle(0);
        let out = t.advance(3 * SEC);
        assert_eq!(out.study, Duration::from_secs(3));
        assert!(out.completed.is_empty());
        assert_eq!(t.time_remaining, Duration::from_secs(7));
    }

    #[test]
    fn work_then_break_then_done() {
        let mut t = timer(10, 5, 1);
        t.toggle(0);
        let out = t.advance(10 * SEC);
        assert_eq!(out.completed, vec![Completion { mode: TimerMode::Work, all_done: false }]);
        assert_eq!(t.timer_mode, TimerMode::Break);
        assert!(t.is_running(), "the break starts by itself");

        let out = t.advance(15 * SEC);
        assert_eq!(out.study, Duration::ZERO, "break time isn't study time");
        assert_eq!(out.completed, vec![Completion { mode: TimerMode::Break, all_done: true }]);
        assert!(!t.is_running(), "after the last loop the timer resets and stops");
        assert_eq!(t.timer_mode, TimerMode::Work);
        assert_eq!(t.time_remaining, Duration::from_secs(10));
    }

    #[test]
    fn catches_up_after_a_long_sleep() {
        // 2 loops of 10s work + 5s break = 30s. Sleep for 100s.
        let mut t = timer(10, 5, 2);
        t.toggle(0);
        let out = t.advance(100 * SEC);
        let modes: Vec<_> = out.completed.iter().map(|c| (c.mode, c.all_done)).collect();
        assert_eq!(
            modes,
            vec![
                (TimerMode::Work, false),
                (TimerMode::Break, false),
                (TimerMode::Work, false),
                (TimerMode::Break, true),
            ]
        );
        assert_eq!(out.study, Duration::from_secs(20));
        assert!(!t.is_running());
    }

    #[test]
    fn leftover_time_carries_into_the_next_session() {
        let mut t = timer(10, 5, 2);
        t.toggle(0);
        let out = t.advance(12 * SEC);
        assert_eq!(out.study, Duration::from_secs(10));
        assert_eq!(t.timer_mode, TimerMode::Break);
        assert_eq!(t.time_remaining, Duration::from_secs(3));
    }

    #[test]
    fn pausing_counts_time_up_to_the_pause() {
        let mut t = timer(10, 5, 1);
        t.toggle(0);
        let out = t.toggle(4 * SEC);
        assert_eq!(out.study, Duration::from_secs(4));
        assert!(!t.is_running());
        // Time while paused doesn't count.
        assert_eq!(t.advance(100 * SEC), TickOutcome::default());
        assert_eq!(t.time_remaining, Duration::from_secs(6));
    }

    #[test]
    fn clock_moving_backwards_is_ignored() {
        let mut t = timer(10, 5, 1);
        t.toggle(50 * SEC);
        let out = t.advance(40 * SEC);
        assert_eq!(out, TickOutcome::default());
        assert_eq!(t.time_remaining, Duration::from_secs(10));
    }

    #[test]
    fn zero_length_sessions_do_not_hang() {
        let mut t = timer(0, 0, 20);
        t.toggle(0);
        let out = t.advance(0);
        assert_eq!(out.completed.len(), 40);
        assert!(!t.is_running());
    }

    #[test]
    fn reset_goes_back_to_the_start() {
        let mut t = timer(60, 10, 2);
        t.toggle(0);
        t.advance(65 * SEC);
        t.reset();
        assert_eq!(t.timer_state, TimerState::Paused);
        assert_eq!(t.timer_mode, TimerMode::Work);
        assert_eq!(t.time_remaining, Duration::from_secs(60));
        assert_eq!(t.current_loop, 1);
    }

    #[test]
    fn upcoming_lists_every_session_end() {
        let mut t = timer(10, 5, 2);
        t.toggle(0);
        t.advance(4 * SEC);
        // Asked 1s after the last update: work ends in 5s, then +5, +10, +5.
        let times: Vec<_> = t.upcoming(5 * SEC).iter().map(|(d, _)| d.as_secs()).collect();
        assert_eq!(times, vec![5, 10, 20, 25]);
        assert!(t.upcoming(5 * SEC).last().unwrap().1.all_done);
        t.toggle(6 * SEC);
        assert!(t.upcoming(6 * SEC).is_empty(), "a paused timer has nothing coming");
    }

    #[test]
    fn progress_goes_from_zero_to_one() {
        let mut t = timer(10, 5, 1);
        assert_eq!(t.progress(), 0.0);
        t.toggle(0);
        t.advance(5 * SEC);
        assert!((t.progress() - 0.5).abs() < 1e-9);
    }

    #[test]
    fn save_and_restore_keeps_a_running_timer() {
        let mut t = timer(10, 5, 2);
        t.toggle(0);
        t.advance(12 * SEC);
        let saved = t.save();

        let mut restored = timer(10, 5, 2);
        restored.restore(&saved);
        assert!(restored.is_running());
        assert_eq!(restored.timer_mode, TimerMode::Break);
        // 17s later (app was closed): the break (3s left) and the next work (10s) are done,
        // and the last break has 1s to go.
        let out = restored.advance(29 * SEC);
        assert_eq!(out.completed.len(), 2);
        assert_eq!(restored.timer_mode, TimerMode::Break);
        assert_eq!(restored.current_loop, 2);
    }

    #[test]
    fn restore_fixes_values_that_no_longer_fit() {
        let saved = TimerSave {
            mode: TimerMode::Work,
            running: true,
            remaining_ms: 999_999,
            current_loop: 9,
            last_update_ms: None,
        };
        let mut t = timer(10, 5, 2);
        t.restore(&saved);
        assert_eq!(t.time_remaining, Duration::from_secs(10));
        assert_eq!(t.current_loop, 2);
        assert!(!t.is_running(), "running without a timestamp can't be trusted");
    }

    #[test]
    fn messages_match_the_session_that_ended() {
        let work = Completion { mode: TimerMode::Work, all_done: false };
        let brk = Completion { mode: TimerMode::Break, all_done: false };
        let last = Completion { mode: TimerMode::Break, all_done: true };
        assert_eq!(work.message().0, "Work Complete!");
        assert_eq!(brk.message().0, "Break Over!");
        assert_eq!(last.message().0, "All Sessions Done!");
    }
}
