use std::time::{Duration, Instant};

#[derive(PartialEq, Clone, Copy, Debug)]
pub enum TimerMode {
    Work,
    Break,
}

#[derive(PartialEq, Clone, Copy, Debug)]
pub enum TimerState {
    Paused,
    Running,
}

pub enum TimerEvent {
    Tick,
    SessionCompleted(TimerMode),
}

pub struct StudyTimer {
    pub work_duration: Duration,
    pub break_duration: Duration,
    pub total_loops: u32,
    pub timer_mode: TimerMode,
    pub timer_state: TimerState,
    pub time_remaining: Duration,
    pub current_loop: u32,
    last_tick: Option<Instant>,
    pub pending_study_time: Duration,
}

impl StudyTimer {
    pub fn new(
        work_duration: Duration,
        break_duration: Duration,
        total_loops: u32,
    ) -> Self {
        Self {
            work_duration,
            break_duration,
            total_loops,
            timer_mode: TimerMode::Work,
            timer_state: TimerState::Paused,
            time_remaining: work_duration,
            current_loop: 1,
            last_tick: None,
            pending_study_time: Duration::ZERO,
        }
    }

    pub fn set_durations(
        &mut self,
        work_duration: Duration,
        break_duration: Duration,
        total_loops: u32,
    ) {
        self.work_duration = work_duration;
        self.break_duration = break_duration;
        self.total_loops = total_loops;
        self.reset();
    }

    pub fn tick(&mut self) -> Option<TimerEvent> {
        if self.timer_state != TimerState::Running {
            return None;
        }

        let now = Instant::now();
        let elapsed = self
            .last_tick
            .map_or(Duration::ZERO, |t| now.duration_since(t));
        self.last_tick = Some(now);



        if self.timer_mode == TimerMode::Work {
            self.pending_study_time += elapsed;
            if self.pending_study_time >= Duration::from_secs(1) {
                // Determine how many whole seconds passed
                // We actually handled the "recording" of stats in `main.rs` now via `TimerEvent::Tick` logic or just polling `pending_study_time`.
                // For simplicity, we can let the caller handle the exact time accumulation, OR return a specific event.
                // But to follow the plan: "Timer calls tick and returns events".
                // We'll update state here, and `main` will read it.
            }
        }

        if self.time_remaining > elapsed {
            self.time_remaining -= elapsed;
            return Some(TimerEvent::Tick); // Valid tick
        } else {
            // Timer finished
            self.time_remaining = Duration::ZERO;
            let finished_mode = self.timer_mode;
            self.switch_session();
            return Some(TimerEvent::SessionCompleted(finished_mode));
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

    pub fn get_session_switch_messages(&self) -> (&'static str, &'static str) {
        match self.timer_mode {
            TimerMode::Work => ("Work Complete!", "Time for a short break."),
            TimerMode::Break => ("Break Over!", "Time to get back to work."),
        }
    }
}
