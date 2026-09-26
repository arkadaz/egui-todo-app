//! Moving data in and out: the desktop app's JSON (import, merge, export) and daily backups.

use super::*;

impl Hub {
    /// Everything as `focushub_data.json` (the desktop app can open it too).
    pub fn export_json(&mut self) -> Result<String> {
        self.reload_if_changed()?;
        self.data.timer = Some(self.timer.save());
        Ok(serde_json::to_string_pretty(&self.data)?)
    }

    fn parse_import(json: &str) -> Result<AppData> {
        let mut imported = database::parse_json(json)?;
        clean_up(&mut imported);
        Ok(imported)
    }

    /// Replaces the tasks, stats and rewards with those from a `focushub_data.json` file
    /// (from this app or the desktop app). Settings, the timer and the background stay.
    pub fn import_json(&mut self, json: &str, now_ms: i64) -> Result<Imported> {
        let imported = Self::parse_import(json)?;
        self.reload_if_changed()?;
        self.data.todos_by_date = imported.todos_by_date;
        self.data.stats = imported.stats;
        self.data.rewards = imported.rewards;
        self.save(now_ms, [Change::Everything])?;
        Ok(Imported {
            days: self.data.todos_by_date.len() as u32,
            tasks: self.data.todos_by_date.values().map(Vec::len).sum::<usize>() as u32,
            rewards: self.data.rewards.len() as u32,
        })
    }

    /// Combines a `focushub_data.json` file with the data already here, for using the
    /// desktop app and the phone together. Nothing is ever counted twice: tasks and rewards
    /// with the same text are kept once (finished if either copy is), and for each day
    /// the larger study time and session count win. Merging the same file again changes
    /// nothing.
    pub fn merge_json(&mut self, json: &str, now_ms: i64) -> Result<Merged> {
        let imported = Self::parse_import(json)?;
        self.reload_if_changed()?;
        let mut merged = Merged {
            tasks_added: 0,
            study_days_updated: 0,
            rewards_added: 0,
        };

        for (day, todos) in imported.todos_by_date {
            let ours = self.data.todos_by_date.entry(day).or_default();
            for todo in todos {
                if let Some(existing) = ours.iter_mut().find(|t| t.text == todo.text) {
                    existing.completed |= todo.completed;
                } else {
                    ours.push(todo);
                    merged.tasks_added += 1;
                }
            }
        }

        let stats = &mut self.data.stats;
        for (day, secs) in imported.stats.daily_study_seconds {
            let ours = stats.daily_study_seconds.entry(day).or_insert(0);
            if secs > *ours {
                *ours = secs;
                merged.study_days_updated += 1;
            }
        }
        for (day, count) in imported.stats.daily_streaks {
            let ours = stats.daily_streaks.entry(day).or_insert(0);
            *ours = (*ours).max(count);
        }
        for (month, count) in imported.stats.monthly_streaks {
            let ours = stats.monthly_streaks.entry(month).or_insert(0);
            *ours = (*ours).max(count);
        }

        for reward in imported.rewards {
            if let Some(existing) = self.data.rewards.iter_mut().find(|r| r.name == reward.name) {
                existing.completed |= reward.completed;
            } else {
                self.data.rewards.push(reward);
                merged.rewards_added += 1;
            }
        }
        sort_rewards(&mut self.data.rewards);
        self.save(now_ms, [Change::Everything])?;
        Ok(merged)
    }

    /// The dates of the daily backups, newest first.
    pub fn backups(&self) -> Vec<NaiveDate> {
        self.db.backups()
    }

    /// Replaces everything (including settings and the timer) with a daily backup.
    pub fn restore_backup(&mut self, date: NaiveDate, now_ms: i64) -> Result<()> {
        let mut data = self.db.read_backup(date)?;
        clean_up(&mut data);
        self.timer = timer_for(&data);
        self.data = data;
        self.study_carry_ms = 0;
        self.pending_completions.clear();
        self.save(now_ms, [Change::Everything])
    }
}
