//! The Pomodoro timer: work sessions and breaks, repeated for a number of loops, with an
//! optional long break every few loops.
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

/// How the timer runs. Durations are clamped by `Hub`; the timer itself accepts anything.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TimerConfig {
    pub work: Duration,
    pub short_break: Duration,
    pub long_break: Duration,
    pub total_loops: u32,
    /// A long break after every this many loops. 0 = never.
    pub long_break_every: u32,
    /// Start the next session automatically when one ends.
    pub auto_start: bool,
}

/// A session that just ended.
#[derive(PartialEq, Eq, Clone, Copy, Debug)]
pub struct Completion {
    /// The kind of session that ended.
    pub mode: TimerMode,
    /// For a finished work session: whether the break that follows is a long one.
    pub next_is_long_break: bool,
    /// True when this was the last break, so every loop is finished.
    pub all_done: bool,
}

impl Completion {
    /// The alert shown when this session ends.
    pub fn message(self) -> (&'static str, &'static str) {
        match (self.mode, self.next_is_long_break, self.all_done) {
            (TimerMode::Work, true, _) => ("Work Complete!", "Time for a long break. You earned it."),
            (TimerMode::Work, false, _) => ("Work Complete!", "Time for a short break."),
            (TimerMode::Break, _, false) => ("Break Over!", "Time to get back to work."),
            (TimerMode::Break, _, true) => ("All Sessions Done!", "Great work! Every loop is complete."),
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
    /// Whether the current break is a long one.
    #[serde(default)]
    pub long_break: bool,
}

#[derive(Clone, Debug)]
pub struct StudyTimer {
    pub config: TimerConfig,
    pub timer_mode: TimerMode,
    /// Whether the current break is a long one.
    pub long_break: bool,
    pub timer_state: TimerState,
    pub time_remaining: Duration,
    pub current_loop: u32,
    last_update_ms: Option<i64>,
}

impl StudyTimer {
    pub fn new(config: TimerConfig) -> Self {
        let mut timer = Self {
            config: TimerConfig {
                total_loops: config.total_loops.max(1),
                ..config
            },
            timer_mode: TimerMode::Work,
            long_break: false,
            timer_state: TimerState::Paused,
            time_remaining: config.work,
            current_loop: 1,
            last_update_ms: None,
        };
        timer.reset();
        timer
    }

    /// Changes how the timer runs, then resets it (like the desktop app).
    pub fn configure(&mut self, config: TimerConfig) {
        self.config = TimerConfig {
            total_loops: config.total_loops.max(1),
            ..config
        };
        self.reset();
    }

    pub fn is_running(&self) -> bool {
        self.timer_state == TimerState::Running
    }

    /// True before the first session has started (nothing to pause, skip or reset).
    pub fn is_at_start(&self) -> bool {
        !self.is_running()
            && self.timer_mode == TimerMode::Work
            && self.current_loop == 1
            && self.time_remaining == self.config.work
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
                self.pause_now();
                outcome
            }
        }
    }

    fn pause_now(&mut self) {
        self.timer_state = TimerState::Paused;
        self.last_update_ms = None;
    }

    pub fn reset(&mut self) {
        self.timer_state = TimerState::Paused;
        self.timer_mode = TimerMode::Work;
        self.long_break = false;
        self.time_remaining = self.config.work;
        self.current_loop = 1;
        self.last_update_ms = None;
    }

    /// Ends the current session right away and moves to the next one. Time already spent
    /// still counts. A running timer keeps running (if auto-start is on).
    pub fn skip(&mut self, now_ms: i64) -> TickOutcome {
        let mut outcome = self.advance(now_ms);
        if outcome.completed.last().is_some_and(|c| c.all_done) {
            return outcome; // everything finished while catching up
        }
        let was_running = self.is_running();
        let completion = self.finish_session();
        outcome.completed.push(completion);
        if !completion.all_done {
            if was_running && self.config.auto_start {
                self.timer_state = TimerState::Running;
                self.last_update_ms = Some(now_ms);
            } else {
                self.pause_now();
            }
        }
        outcome
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
            let completion = self.finish_session();
            outcome.completed.push(completion);
            if completion.all_done {
                break;
            }
            if !self.config.auto_start {
                // Wait at the start of the next session until the user presses Start.
                self.pause_now();
                break;
            }
        }
        outcome
    }

    /// Moves to the next session and describes the one that ended. Resets the timer if
    /// every loop is done.
    fn finish_session(&mut self) -> Completion {
        match self.timer_mode {
            TimerMode::Work => {
                let every = self.config.long_break_every;
                self.long_break = every > 0 && self.current_loop.is_multiple_of(every);
                self.timer_mode = TimerMode::Break;
                self.time_remaining = self.break_duration();
                Completion {
                    mode: TimerMode::Work,
                    next_is_long_break: self.long_break,
                    all_done: false,
                }
            }
            TimerMode::Break => {
                let all_done = self.current_loop >= self.config.total_loops;
                if all_done {
                    self.reset();
                } else {
                    self.current_loop += 1;
                    self.timer_mode = TimerMode::Work;
                    self.long_break = false;
                    self.time_remaining = self.config.work;
                }
                Completion {
                    mode: TimerMode::Break,
                    next_is_long_break: false,
                    all_done,
                }
            }
        }
    }

    fn break_duration(&self) -> Duration {
        if self.long_break {
            self.config.long_break
        } else {
            self.config.short_break
        }
    }

    /// Every session end still to come if the timer keeps running, with how long from
    /// `now_ms` until each one, and the timer as it will be right after. Used to schedule
    /// notifications ahead of time. With auto-start off, only the current session's end.
    pub fn upcoming(&self, now_ms: i64) -> Vec<(Duration, Completion, StudyTimer)> {
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
            let completion = sim.finish_session();
            let stops = completion.all_done || !sim.config.auto_start;
            if !completion.all_done && !sim.config.auto_start {
                sim.pause_now();
            }
            list.push((at.saturating_sub(since_update), completion, sim.clone()));
            if stops {
                break;
            }
        }
        list
    }

    /// The length of the current session.
    pub fn session_duration(&self) -> Duration {
        match self.timer_mode {
            TimerMode::Work => self.config.work,
            TimerMode::Break => self.break_duration(),
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
            long_break: self.long_break,
        }
    }

    /// Restores a saved state, fixing anything that doesn't fit the current settings.
    pub fn restore(&mut self, saved: &TimerSave) {
        self.timer_mode = saved.mode;
        self.long_break = saved.mode == TimerMode::Break && saved.long_break;
        self.current_loop = saved.current_loop.clamp(1, self.config.total_loops);
        self.time_remaining = Duration::from_millis(saved.remaining_ms).min(self.session_duration());
        match (saved.running, saved.last_update_ms) {
            (true, Some(last)) => {
                self.timer_state = TimerState::Running;
                self.last_update_ms = Some(last);
            }
            _ => self.pause_now(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SEC: i64 = 1000;

    fn config(work: u64, brk: u64, loops: u32) -> TimerConfig {
        TimerConfig {
            work: Duration::from_secs(work),
            short_break: Duration::from_secs(brk),
            long_break: Duration::from_secs(brk * 3),
            total_loops: loops,
            long_break_every: 0,
            auto_start: true,
        }
    }

    fn timer(work: u64, brk: u64, loops: u32) -> StudyTimer {
        StudyTimer::new(config(work, brk, loops))
    }

    fn modes(out: &TickOutcome) -> Vec<(TimerMode, bool)> {
        out.completed.iter().map(|c| (c.mode, c.all_done)).collect()
    }

    #[test]
    fn starts_paused_on_work() {
        let t = timer(1500, 300, 4);
        assert_eq!(t.timer_mode, TimerMode::Work);
        assert_eq!(t.timer_state, TimerState::Paused);
        assert_eq!(t.time_remaining, Duration::from_secs(1500));
        assert_eq!(t.current_loop, 1);
        assert!(t.is_at_start());
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
        assert!(!t.is_at_start());
    }

    #[test]
    fn work_then_break_then_done() {
        let mut t = timer(10, 5, 1);
        t.toggle(0);
        let out = t.advance(10 * SEC);
        assert_eq!(modes(&out), vec![(TimerMode::Work, false)]);
        assert_eq!(t.timer_mode, TimerMode::Break);
        assert!(t.is_running(), "the break starts by itself");

        let out = t.advance(15 * SEC);
        assert_eq!(out.study, Duration::ZERO, "break time isn't study time");
        assert_eq!(modes(&out), vec![(TimerMode::Break, true)]);
        assert!(!t.is_running(), "after the last loop the timer resets and stops");
        assert!(t.is_at_start());
    }

    #[test]
    fn catches_up_after_a_long_sleep() {
        // 2 loops of 10s work + 5s break = 30s. Sleep for 100s.
        let mut t = timer(10, 5, 2);
        t.toggle(0);
        let out = t.advance(100 * SEC);
        assert_eq!(
            modes(&out),
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
        assert_eq!(t.advance(100 * SEC), TickOutcome::default());
        assert_eq!(t.time_remaining, Duration::from_secs(6));
    }

    #[test]
    fn clock_moving_backwards_is_ignored() {
        let mut t = timer(10, 5, 1);
        t.toggle(50 * SEC);
        assert_eq!(t.advance(40 * SEC), TickOutcome::default());
        assert_eq!(t.time_remaining, Duration::from_secs(10));
    }

    #[test]
    fn zero_length_sessions_do_not_hang() {
        let mut t = timer(0, 0, 20);
        t.toggle(0);
        assert_eq!(t.advance(0).completed.len(), 40);
        assert!(!t.is_running());
    }

    #[test]
    fn reset_goes_back_to_the_start() {
        let mut t = timer(60, 10, 2);
        t.toggle(0);
        t.advance(65 * SEC);
        t.reset();
        assert!(t.is_at_start());
    }

    #[test]
    fn long_break_every_few_loops() {
        let mut t = StudyTimer::new(TimerConfig {
            long_break_every: 2,
            ..config(10, 5, 4)
        });
        t.toggle(0);
        let out = t.advance(10 * SEC); // loop 1 work done -> short break
        assert!(!out.completed[0].next_is_long_break);
        assert!(!t.long_break);
        assert_eq!(t.time_remaining, Duration::from_secs(5));
        t.advance(15 * SEC); // short break done -> loop 2 work
        let out = t.advance(25 * SEC); // loop 2 work done -> long break (every 2)
        assert!(out.completed[0].next_is_long_break);
        assert!(t.long_break);
        assert_eq!(t.time_remaining, Duration::from_secs(15));
        assert_eq!(out.completed[0].message().1, "Time for a long break. You earned it.");
        t.advance(40 * SEC); // long break done -> loop 3 work, no longer long
        assert!(!t.long_break);
        assert_eq!(t.current_loop, 3);
    }

    #[test]
    fn without_auto_start_the_timer_waits_between_sessions() {
        let mut t = StudyTimer::new(TimerConfig {
            auto_start: false,
            ..config(10, 5, 2)
        });
        t.toggle(0);
        let out = t.advance(100 * SEC);
        assert_eq!(
            modes(&out),
            vec![(TimerMode::Work, false)],
            "only the running session ends"
        );
        assert!(!t.is_running());
        assert_eq!(t.timer_mode, TimerMode::Break);
        assert_eq!(t.time_remaining, Duration::from_secs(5), "the break waits, untouched");
        assert_eq!(out.study, Duration::from_secs(10));
    }

    #[test]
    fn skip_ends_the_current_session() {
        let mut t = timer(10, 5, 2);
        t.toggle(0);
        let out = t.skip(3 * SEC);
        assert_eq!(out.study, Duration::from_secs(3), "time spent still counts");
        assert_eq!(modes(&out), vec![(TimerMode::Work, false)]);
        assert_eq!(t.timer_mode, TimerMode::Break);
        assert!(t.is_running(), "keeps running into the break");
        assert_eq!(t.time_remaining, Duration::from_secs(5));

        let out = t.skip(4 * SEC); // skip the break
        assert_eq!(modes(&out), vec![(TimerMode::Break, false)]);
        assert_eq!(t.current_loop, 2);
        assert_eq!(
            t.advance(6 * SEC).study,
            Duration::from_secs(2),
            "counting from the skip"
        );
    }

    #[test]
    fn skip_while_paused_stays_paused() {
        let mut t = timer(10, 5, 1);
        t.skip(0);
        assert_eq!(t.timer_mode, TimerMode::Break);
        assert!(!t.is_running());
        let out = t.skip(0);
        assert_eq!(modes(&out), vec![(TimerMode::Break, true)]);
        assert!(t.is_at_start());
    }

    #[test]
    fn upcoming_lists_every_session_end() {
        let mut t = timer(10, 5, 2);
        t.toggle(0);
        t.advance(4 * SEC);
        // Asked 1s after the last update: work ends in 5s, then +5, +10, +5.
        let list = t.upcoming(5 * SEC);
        let times: Vec<_> = list.iter().map(|(d, _, _)| d.as_secs()).collect();
        assert_eq!(times, vec![5, 10, 20, 25]);
        assert!(list.last().unwrap().1.all_done);
        let (_, _, after_first) = &list[0];
        assert_eq!(
            after_first.timer_mode,
            TimerMode::Break,
            "the state right after each end"
        );
        t.toggle(6 * SEC);
        assert!(t.upcoming(6 * SEC).is_empty(), "a paused timer has nothing coming");
    }

    #[test]
    fn upcoming_stops_at_the_first_end_without_auto_start() {
        let mut t = StudyTimer::new(TimerConfig {
            auto_start: false,
            ..config(10, 5, 2)
        });
        t.toggle(0);
        let list = t.upcoming(0);
        assert_eq!(list.len(), 1);
        assert!(!list[0].2.is_running(), "after it, the timer waits");
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
        let mut t = StudyTimer::new(TimerConfig {
            long_break_every: 1,
            ..config(10, 5, 2)
        });
        t.toggle(0);
        t.advance(12 * SEC); // in a long break (every loop), 13s left of 15
        let saved = t.save();
        assert!(saved.long_break);

        let mut restored = StudyTimer::new(TimerConfig {
            long_break_every: 1,
            ..config(10, 5, 2)
        });
        restored.restore(&saved);
        assert!(restored.is_running());
        assert!(restored.long_break);
        assert_eq!(restored.time_remaining, Duration::from_secs(13));
        let out = restored.advance(25 * SEC + 500); // break ends, next work 0.5s in
        assert_eq!(out.completed.len(), 1);
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
            long_break: true,
        };
        let mut t = timer(10, 5, 2);
        t.restore(&saved);
        assert_eq!(t.time_remaining, Duration::from_secs(10));
        assert_eq!(t.current_loop, 2);
        assert!(!t.long_break, "only a break can be long");
        assert!(!t.is_running(), "running without a timestamp can't be trusted");
    }

    #[test]
    fn messages_match_the_session_that_ended() {
        let c = |mode, next_is_long_break, all_done| Completion {
            mode,
            next_is_long_break,
            all_done,
        };
        assert_eq!(c(TimerMode::Work, false, false).message().0, "Work Complete!");
        assert_eq!(c(TimerMode::Break, false, false).message().0, "Break Over!");
        assert_eq!(c(TimerMode::Break, false, true).message().0, "All Sessions Done!");
    }
}
