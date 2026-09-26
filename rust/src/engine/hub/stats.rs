//! Stats: study time, sessions, streaks and the best day.

use super::*;

impl Hub {
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
        self.data
            .stats
            .monthly_streaks
            .get(&month_key(day))
            .copied()
            .unwrap_or(0)
    }

    /// Study seconds for each of the `days` days ending with `last`, oldest first.
    pub fn study_series(&self, last: NaiveDate, days: u32) -> Vec<(NaiveDate, u64)> {
        (0..i64::from(days))
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
        let mut day = if self.studied(today) {
            today
        } else {
            today - Days::days(1)
        };
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
}
