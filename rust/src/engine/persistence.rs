//! Saving and loading `focushub_data.json`.

use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};

use super::domain::AppData;

pub const FILE_NAME: &str = "focushub_data.json";

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
    pub fn save(&self, data: &AppData) -> Result<()> {
        if let Some(dir) = self.path.parent() {
            fs::create_dir_all(dir)?;
        }
        let temp = self.path.with_extension("json.tmp");
        fs::write(&temp, serde_json::to_string_pretty(data)?)
            .with_context(|| format!("Couldn't write {}", temp.display()))?;
        fs::rename(&temp, &self.path)
            .with_context(|| format!("Couldn't replace {}", self.path.display()))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::domain::{Reward, TodoItem};
    use chrono::NaiveDate;

    #[test]
    fn missing_file_is_a_fresh_start() {
        let dir = tempfile::tempdir().unwrap();
        let (data, warning) = JsonStore::in_dir(dir.path()).load(0).unwrap();
        assert_eq!(data, AppData::default());
        assert!(warning.is_none());
    }

    #[test]
    fn save_then_load_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let store = JsonStore::in_dir(dir.path());
        let mut data = AppData::default();
        let day = NaiveDate::from_ymd_opt(2026, 9, 26).unwrap();
        data.todos_by_date.insert(day, vec![TodoItem { text: "Read".into(), completed: true }]);
        data.rewards.push(Reward { name: "Cake".into(), completed: false });
        data.stats.daily_study_seconds.insert(day, 125);
        store.save(&data).unwrap();
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
        let day = NaiveDate::from_ymd_opt(2025, 7, 25).unwrap();
        assert_eq!(data.todos_by_date[&day][0].text, "Study Rust");
        assert_eq!(data.stats.daily_study_seconds[&day], 3600);
        assert_eq!(data.stats.monthly_streaks["2025-7"], 5);
        assert_eq!(data.rewards[0].name, "Ice cream");
        assert_eq!(data.settings.work_secs, 3600, "missing settings use the defaults");
        assert!(data.timer.is_none());
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
}
