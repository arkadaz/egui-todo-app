//! Saving and loading `focushub_data.json`, plus daily backups.

use anyhow::{bail, Context, Result};
use chrono::NaiveDate;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use super::domain::AppData;

pub const FILE_NAME: &str = "focushub_data.json";
const BACKUPS_DIR: &str = "backups";
/// How many daily backups to keep.
const KEEP_BACKUPS: usize = 7;

/// What the file looked like when we last read or wrote it. If it changes, someone else
/// wrote it (the notification buttons run in their own copy of the app).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DiskStamp {
    modified: SystemTime,
    len: u64,
}

pub struct JsonStore {
    path: PathBuf,
}

impl JsonStore {
    pub fn in_dir(dir: &Path) -> Self {
        Self { path: dir.join(FILE_NAME) }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The file's current stamp, or `None` if it doesn't exist.
    pub fn stamp(&self) -> Option<DiskStamp> {
        let meta = fs::metadata(&self.path).ok()?;
        Some(DiskStamp { modified: meta.modified().ok()?, len: meta.len() })
    }

    /// Loads the saved data. A missing file means a fresh start.
    ///
    /// A file that can't be read as JSON is never overwritten: it's renamed to a backup,
    /// the app starts fresh, and the returned message explains what happened.
    pub fn load(&self, now_ms: i64) -> Result<(AppData, Option<String>)> {
        if !self.path.exists() {
            return Ok((AppData::default(), None));
        }
        let text = fs::read_to_string(&self.path)
            .with_context(|| format!("Couldn't read {}", self.path.display()))?;
        match serde_json::from_str(&text) {
            Ok(data) => Ok((data, None)),
            Err(error) => {
                let backup = self.path.with_file_name(format!("focushub_data.damaged-{now_ms}.json"));
                fs::rename(&self.path, &backup)
                    .with_context(|| format!("Couldn't back up the damaged file {}", self.path.display()))?;
                let name = backup.file_name().unwrap_or_default().to_string_lossy();
                Ok((
                    AppData::default(),
                    Some(format!(
                        "Your saved data couldn't be read ({error}). It was kept as {name} and the app started fresh."
                    )),
                ))
            }
        }
    }

    /// Saves safely: writes a temporary file, then swaps it in, so a crash in the
    /// middle of saving can't leave a half-written file behind.
    pub fn save(&self, data: &AppData) -> Result<Option<DiskStamp>> {
        if let Some(dir) = self.path.parent() {
            fs::create_dir_all(dir)?;
        }
        let temp = self.path.with_extension("json.tmp");
        fs::write(&temp, serde_json::to_string_pretty(data)?)
            .with_context(|| format!("Couldn't write {}", temp.display()))?;
        fs::rename(&temp, &self.path)
            .with_context(|| format!("Couldn't replace {}", self.path.display()))?;
        Ok(self.stamp())
    }

    fn backups_dir(&self) -> PathBuf {
        self.path.with_file_name(BACKUPS_DIR)
    }

    /// Copies the saved file to `backups/focushub_data-YYYY-MM-DD.json`, once per day,
    /// and deletes all but the newest few backups.
    pub fn back_up_daily(&self, today: NaiveDate) -> Result<()> {
        if !self.path.exists() {
            return Ok(());
        }
        let dir = self.backups_dir();
        fs::create_dir_all(&dir)?;
        let target = dir.join(format!("focushub_data-{}.json", today.format("%Y-%m-%d")));
        if !target.exists() {
            fs::copy(&self.path, &target)
                .with_context(|| format!("Couldn't write the backup {}", target.display()))?;
        }
        for old in self.backups().into_iter().skip(KEEP_BACKUPS) {
            let _ = fs::remove_file(dir.join(format!("focushub_data-{}.json", old.format("%Y-%m-%d"))));
        }
        Ok(())
    }

    /// The dates of the available backups, newest first.
    pub fn backups(&self) -> Vec<NaiveDate> {
        let mut dates: Vec<NaiveDate> = fs::read_dir(self.backups_dir())
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|entry| {
                let name = entry.file_name().to_string_lossy().to_string();
                let date = name.strip_prefix("focushub_data-")?.strip_suffix(".json")?;
                NaiveDate::parse_from_str(date, "%Y-%m-%d").ok()
            })
            .collect();
        dates.sort_unstable_by(|a, b| b.cmp(a));
        dates
    }

    /// Reads a backup's data.
    pub fn read_backup(&self, date: NaiveDate) -> Result<AppData> {
        let path = self.backups_dir().join(format!("focushub_data-{}.json", date.format("%Y-%m-%d")));
        if !path.exists() {
            bail!("There's no backup from {date}.");
        }
        let text = fs::read_to_string(&path).with_context(|| format!("Couldn't read {}", path.display()))?;
        serde_json::from_str(&text).context("That backup can't be read.")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::domain::{Reward, TodoItem};

    fn day(d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 9, d).unwrap()
    }

    #[test]
    fn missing_file_is_a_fresh_start() {
        let dir = tempfile::tempdir().unwrap();
        let store = JsonStore::in_dir(dir.path());
        let (data, warning) = store.load(0).unwrap();
        assert_eq!(data, AppData::default());
        assert!(warning.is_none());
        assert!(store.stamp().is_none());
    }

    #[test]
    fn save_then_load_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let store = JsonStore::in_dir(dir.path());
        let mut data = AppData::default();
        data.todos_by_date.insert(day(26), vec![TodoItem { text: "Read".into(), completed: true }]);
        data.rewards.push(Reward { name: "Cake".into(), completed: false });
        data.stats.daily_study_seconds.insert(day(26), 125);
        let stamp = store.save(&data).unwrap();
        assert_eq!(stamp, store.stamp());
        let (loaded, warning) = store.load(0).unwrap();
        assert_eq!(loaded, data);
        assert!(warning.is_none());
        assert!(!dir.path().join("focushub_data.json.tmp").exists());
    }

    #[test]
    fn reads_a_file_from_the_desktop_app() {
        // Exactly the shape the egui app writes (no `settings` or `timer` sections).
        let json = r#"{
          "todos_by_date": { "2025-07-25": [ { "text": "Study Rust", "completed": false } ] },
          "stats": {
            "daily_study_seconds": { "2025-07-25": 3600 },
            "daily_streaks": { "2025-07-25": 2 },
            "monthly_streaks": { "2025-7": 5 }
          },
          "rewards": [ { "name": "Ice cream", "completed": true } ],
          "gif_path": "C:\\Users\\someone\\duck.gif"
        }"#;
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join(FILE_NAME), json).unwrap();
        let (data, warning) = JsonStore::in_dir(dir.path()).load(0).unwrap();
        assert!(warning.is_none());
        let date = NaiveDate::from_ymd_opt(2025, 7, 25).unwrap();
        assert_eq!(data.todos_by_date[&date][0].text, "Study Rust");
        assert_eq!(data.stats.daily_study_seconds[&date], 3600);
        assert_eq!(data.stats.monthly_streaks["2025-7"], 5);
        assert_eq!(data.rewards[0].name, "Ice cream");
        assert_eq!(data.settings.work_secs, 3600, "missing settings use the defaults");
        assert!(data.settings.auto_start);
        assert_eq!(data.settings.long_break_every, 0);
        assert!(data.timer.is_none());
    }

    #[test]
    fn reads_a_file_from_the_first_mobile_version() {
        // Settings without the long-break and auto-start fields.
        let json = r#"{ "todos_by_date": {}, "stats": {}, "rewards": [],
            "settings": { "work_secs": 1500, "break_secs": 300, "loops": 4, "gmt_offset_hours": 7 } }"#;
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join(FILE_NAME), json).unwrap();
        let (data, _) = JsonStore::in_dir(dir.path()).load(0).unwrap();
        assert_eq!(data.settings.work_secs, 1500);
        assert_eq!(data.settings.gmt_offset_hours, Some(7));
        assert_eq!(data.settings.long_break_secs, 15 * 60);
        assert!(data.settings.auto_start);
    }

    #[test]
    fn a_damaged_file_is_kept_as_a_backup() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join(FILE_NAME), "{ this is not json").unwrap();
        let (data, warning) = JsonStore::in_dir(dir.path()).load(1234).unwrap();
        assert_eq!(data, AppData::default());
        assert!(warning.unwrap().contains("focushub_data.damaged-1234.json"));
        let backup = fs::read_to_string(dir.path().join("focushub_data.damaged-1234.json")).unwrap();
        assert_eq!(backup, "{ this is not json");
    }

    #[test]
    fn daily_backups_keep_the_newest_seven() {
        let dir = tempfile::tempdir().unwrap();
        let store = JsonStore::in_dir(dir.path());
        store.back_up_daily(day(1)).unwrap(); // no file yet: nothing to back up
        assert!(store.backups().is_empty());

        let mut data = AppData::default();
        for d in 1..=10 {
            data.rewards = vec![Reward { name: format!("day {d}"), completed: false }];
            store.save(&data).unwrap();
            store.back_up_daily(day(d)).unwrap();
            store.back_up_daily(day(d)).unwrap(); // twice a day: still one backup
        }
        let backups = store.backups();
        assert_eq!(backups, (4..=10).rev().map(day).collect::<Vec<_>>());
        assert_eq!(store.read_backup(day(4)).unwrap().rewards[0].name, "day 4");
        assert!(store.read_backup(day(1)).is_err());
    }
}
