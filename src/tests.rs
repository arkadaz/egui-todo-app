
#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use crate::timer::{StudyTimer, TimerMode, TimerState};
    use crate::domain::{TodoItem, Stats};

    #[test]
    fn test_todo_item_creation() {
        let item = TodoItem {
            text: "Do laundry".to_string(),
            completed: false,
        };
        assert_eq!(item.text, "Do laundry");
        assert!(!item.completed);
    }

    #[test]
    fn test_timer_initialization() {
        let timer = StudyTimer::new(
            Duration::from_secs(1500),
            Duration::from_secs(300),
            4
        );
        assert_eq!(timer.timer_mode, TimerMode::Work);
        assert_eq!(timer.timer_state, TimerState::Paused);
        assert_eq!(timer.time_remaining, Duration::from_secs(1500));
    }

    #[test]
    fn test_timer_tick() {
        let mut timer = StudyTimer::new(
            Duration::from_secs(10),
            Duration::from_secs(5),
            1
        );
        
        // Should not tick while paused
        assert!(timer.tick().is_none());

        timer.toggle_state(); // Running
        assert_eq!(timer.timer_state, TimerState::Running);

        // First tick usually just sets start time, but let's see implementation.
        // Implementation: 
        // let elapsed = last_tick.map_or(ZERO...);
        // last_tick = Some(now);
        // So first tick has 0 elapsed.
        assert!(timer.tick().is_some()); 
    }
    
    #[test]
    fn test_timer_reset() {
         let mut timer = StudyTimer::new(
            Duration::from_secs(60), 
            Duration::from_secs(10),
            2
        );
        timer.toggle_state();
        timer.reset();
        assert_eq!(timer.timer_state, TimerState::Paused);
        assert_eq!(timer.time_remaining, Duration::from_secs(60));
    }
    
    #[test]
    fn test_stats_structure() {
        let mut stats = Stats::default();
        let today = chrono::Local::now().date_naive();
        stats.daily_study_seconds.insert(today, 3600);
        assert_eq!(stats.daily_study_seconds.get(&today), Some(&3600));
    }
}
