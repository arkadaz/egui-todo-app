//! Where the data lives: a `SQLite` database, `focushub.db`, in the app's private folder.
//!
//! The [`Hub`](super::hub::Hub) keeps all the data in memory (it's small) and, after each
//! change, writes only what the change touched, in one transaction. Two copies of the app
//! can be running (the app, and the short-lived copy Android starts for a notification
//! button); each has its own connection, and [`Database::changed_elsewhere`] tells one
//! when the other has written.
//!
//! The tables are created and upgraded by numbered [`MIGRATIONS`], and `SQLite`'s
//! `user_version` remembers which ones have run. On the first run, the data of earlier
//! versions (the JSON file `focushub_data.json`) is moved into the database.

use anyhow::{bail, Context, Result};
use chrono::NaiveDate;
use rusqlite::{params, Connection, OptionalExtension, Transaction, TransactionBehavior};
use std::collections::{BTreeSet, HashMap};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};
use std::time::Duration;

use super::dates::month_key;
use super::domain::{AppData, Reward, Settings, TodoItem};
use super::timer::{TimerMode, TimerSave};

pub const FILE_NAME: &str = "focushub.db";
/// The data file of earlier versions of the app, and of the desktop app.
pub const LEGACY_JSON: &str = "focushub_data.json";
/// What the legacy file is renamed to once its data is in the database.
pub const LEGACY_JSON_MOVED: &str = "focushub_data.before-database.json";
const BACKUPS_DIR: &str = "backups";
/// How many daily backups to keep.
const KEEP_BACKUPS: usize = 7;
/// How long to wait for the other copy of the app to finish writing.
const BUSY_TIMEOUT: Duration = Duration::from_secs(5);

/// Schema changes, oldest first: step `n` (counting from 1) upgrades a database from
/// version `n - 1` to version `n`. Never edit a step that has shipped; add a new one.
pub const MIGRATIONS: &[&str] = &[
    // 1: the first schema.
    "
    CREATE TABLE todos (
        date      TEXT    NOT NULL,                   -- 2026-09-26
        position  INTEGER NOT NULL,                   -- order within the day, from 0
        text      TEXT    NOT NULL,
        completed INTEGER NOT NULL DEFAULT 0 CHECK (completed IN (0, 1)),
        PRIMARY KEY (date, position)
    ) WITHOUT ROWID;

    CREATE TABLE daily_stats (
        date          TEXT    PRIMARY KEY,
        study_seconds INTEGER NOT NULL DEFAULT 0,
        sessions      INTEGER NOT NULL DEFAULT 0      -- completed study sessions
    ) WITHOUT ROWID;

    CREATE TABLE monthly_sessions (
        month    TEXT    PRIMARY KEY,                 -- 2026-9, as the desktop app keys it
        sessions INTEGER NOT NULL
    ) WITHOUT ROWID;

    CREATE TABLE rewards (
        position  INTEGER PRIMARY KEY,                -- unfinished ones first
        name      TEXT    NOT NULL,
        completed INTEGER NOT NULL DEFAULT 0 CHECK (completed IN (0, 1))
    );

    -- Single-row tables (id = 1).
    CREATE TABLE settings (
        id                 INTEGER PRIMARY KEY CHECK (id = 1),
        work_secs          INTEGER NOT NULL,
        break_secs         INTEGER NOT NULL,
        long_break_secs    INTEGER NOT NULL,
        loops              INTEGER NOT NULL,
        long_break_every   INTEGER NOT NULL,          -- 0: no long breaks
        auto_start         INTEGER NOT NULL,
        gmt_offset_hours   INTEGER,                   -- NULL: follow the device
        animate_background INTEGER NOT NULL,
        home_icon_offered  INTEGER NOT NULL,
        background_path    TEXT                       -- NULL: the built-in GIF
    );

    CREATE TABLE timer (
        id             INTEGER PRIMARY KEY CHECK (id = 1),
        mode           TEXT    NOT NULL CHECK (mode IN ('work', 'break')),
        long_break     INTEGER NOT NULL,
        running        INTEGER NOT NULL,
        remaining_ms   INTEGER NOT NULL,
        current_loop   INTEGER NOT NULL,
        last_update_ms INTEGER                        -- when a running timer was last updated
    );
    ",
];

/// What a change touched, so only that is written.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Change {
    /// One day's task list.
    Todos(NaiveDate),
    /// One day's study time and sessions, and its month's session count.
    Stats(NaiveDate),
    Rewards,
    /// The settings and the background.
    Settings,
    /// Everything (imports and restoring a backup).
    Everything,
}

pub struct Database {
    /// In a Mutex only so the Hub can be shared with Flutter's threads (a Connection can't
    /// be): Flutter already lets one call at a time in, so it's never contended.
    conn: Mutex<Connection>,
    path: PathBuf,
    /// `SQLite`'s count of commits made by other connections, when this one last read.
    data_version: i64,
}

/// What [`Database::open`] found.
pub struct Opened {
    pub db: Database,
    pub data: AppData,
    /// A message to show once, if saved data couldn't be read.
    pub warning: Option<String>,
}

impl Database {
    /// Opens the database in `dir`, creating or upgrading it as needed. A damaged file is
    /// never overwritten: it's renamed, the app starts fresh, and `warning` says so.
    pub fn open(dir: &Path, now_ms: i64, today: NaiveDate) -> Result<Opened> {
        fs::create_dir_all(dir).with_context(|| format!("Couldn't create the data folder {}", dir.display()))?;
        let path = dir.join(FILE_NAME);
        let mut warning = None;
        let mut conn = match connect(&path) {
            Ok(conn) => conn,
            Err(error) => {
                let kept = set_aside(&path, now_ms)?;
                warning = Some(format!(
                    "Your saved data couldn't be read ({error:#}). It was kept as {kept} and the app started fresh."
                ));
                connect(&path)?
            }
        };

        let previous_version = migrate(&mut conn, MIGRATIONS)?;
        let mut db = Database {
            conn: Mutex::new(conn),
            path,
            data_version: 0,
        };
        if previous_version == 0 {
            warning = warning.or(db.move_in_legacy_json(dir, now_ms)?);
        } else if warning.is_none() {
            // A backup failing must never stop the app from opening.
            let _ = db.back_up_daily(today);
        }
        let data = db.load()?;
        Ok(Opened { db, data, warning })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    fn conn(&self) -> MutexGuard<'_, Connection> {
        self.conn.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Whether the other copy of the app has written since this one last read.
    pub fn changed_elsewhere(&self) -> Result<bool> {
        Ok(data_version(&self.conn())? != self.data_version)
    }

    /// Reads everything.
    pub fn load(&mut self) -> Result<AppData> {
        let conn = self.conn.get_mut().unwrap_or_else(|poisoned| poisoned.into_inner());
        let data = read_all(conn)?;
        self.data_version = data_version(conn)?;
        Ok(data)
    }

    /// Writes the parts of `data` that `changes` touched, and the timer, in one transaction.
    pub fn write(&mut self, data: &AppData, changes: &BTreeSet<Change>) -> Result<()> {
        let conn = self.conn.get_mut().unwrap_or_else(|poisoned| poisoned.into_inner());
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        if changes.contains(&Change::Everything) {
            write_everything(&tx, data)?;
        } else {
            for change in changes {
                match *change {
                    Change::Todos(date) => write_todos(&tx, data, date)?,
                    Change::Stats(date) => write_stats(&tx, data, date)?,
                    Change::Rewards => write_rewards(&tx, data)?,
                    Change::Settings => write_settings(&tx, data)?,
                    Change::Everything => unreachable!("handled above"),
                }
            }
        }
        write_timer(&tx, data.timer.as_ref())?;
        tx.commit().context("Couldn't save your data")?;
        self.data_version = data_version(conn)?;
        Ok(())
    }

    fn backups_dir(&self) -> PathBuf {
        self.path.with_file_name(BACKUPS_DIR)
    }

    fn backup_path(&self, date: NaiveDate) -> PathBuf {
        self.backups_dir()
            .join(format!("focushub-{}.db", date.format("%Y-%m-%d")))
    }

    /// Copies the database to `backups/focushub-YYYY-MM-DD.db`, once per day, and deletes
    /// all but the newest few backups.
    pub fn back_up_daily(&self, today: NaiveDate) -> Result<()> {
        fs::create_dir_all(self.backups_dir())?;
        let target = self.backup_path(today);
        if !target.exists() {
            self.conn()
                .execute("VACUUM INTO ?1", [target.to_string_lossy()])
                .with_context(|| format!("Couldn't write the backup {}", target.display()))?;
        }
        for old in self.backups().into_iter().skip(KEEP_BACKUPS) {
            let _ = fs::remove_file(self.backup_path(old));
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
                let date = name.strip_prefix("focushub-")?.strip_suffix(".db")?;
                NaiveDate::parse_from_str(date, "%Y-%m-%d").ok()
            })
            .collect();
        dates.sort_unstable_by(|a, b| b.cmp(a));
        dates
    }

    /// Reads a backup's data. The backup file itself is left untouched: it's copied into
    /// memory and upgraded there if it's from an older version.
    pub fn read_backup(&self, date: NaiveDate) -> Result<AppData> {
        let path = self.backup_path(date);
        if !path.exists() {
            bail!("There's no backup from {date}.");
        }
        let mut copy = Connection::open_in_memory()?;
        copy.restore(rusqlite::MAIN_DB, &path, None::<fn(rusqlite::backup::Progress)>)
            .context("That backup can't be read.")?;
        migrate(&mut copy, MIGRATIONS)?;
        read_all(&copy)
    }

    /// Moves the data of an earlier version (`focushub_data.json`) into the new database,
    /// then renames the file so it's never read again. Returns a warning if it was damaged.
    fn move_in_legacy_json(&mut self, dir: &Path, now_ms: i64) -> Result<Option<String>> {
        let legacy = dir.join(LEGACY_JSON);
        if !legacy.exists() {
            return Ok(None);
        }
        let (data, warning) = read_legacy_json(&legacy, now_ms)?;
        if warning.is_none() {
            self.write(&data, &BTreeSet::from([Change::Everything]))?;
            fs::rename(&legacy, dir.join(LEGACY_JSON_MOVED))
                .with_context(|| format!("Couldn't rename {}", legacy.display()))?;
        }
        Ok(warning)
    }
}

/// Parses a `focushub_data.json` (this app's export, or the desktop app's data file).
pub fn parse_json(json: &str) -> Result<AppData> {
    serde_json::from_str(json).context("That file isn't Focus Hub data (focushub_data.json).")
}

/// Reads a legacy data file. One that can't be read is renamed (never overwritten) and
/// the returned message explains what happened.
fn read_legacy_json(path: &Path, now_ms: i64) -> Result<(AppData, Option<String>)> {
    let text = fs::read_to_string(path).with_context(|| format!("Couldn't read {}", path.display()))?;
    match parse_json(&text) {
        Ok(data) => Ok((data, None)),
        Err(error) => {
            let kept = path.with_file_name(format!("focushub_data.damaged-{now_ms}.json"));
            fs::rename(path, &kept).with_context(|| format!("Couldn't set aside {}", path.display()))?;
            let name = kept.file_name().unwrap_or_default().to_string_lossy();
            let reason = error.root_cause();
            Ok((
                AppData::default(),
                Some(format!(
                    "Your saved data couldn't be read ({reason}). It was kept as {name} and the app started fresh."
                )),
            ))
        }
    }
}

/// Opens a connection and checks the file really is a healthy database.
fn connect(path: &Path) -> Result<Connection> {
    let conn = Connection::open(path).with_context(|| format!("Couldn't open {}", path.display()))?;
    conn.busy_timeout(BUSY_TIMEOUT)?;
    // WAL: readers don't wait for the writer, and saving is a quick append.
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "synchronous", "NORMAL")?;
    let check: String = conn.pragma_query_value(None, "quick_check", |row| row.get(0))?;
    if check != "ok" {
        bail!("the database is damaged: {check}");
    }
    Ok(conn)
}

/// Renames a damaged database (and its journal files) out of the way. Returns its new name.
fn set_aside(path: &Path, now_ms: i64) -> Result<String> {
    let name = format!("focushub.damaged-{now_ms}.db");
    fs::rename(path, path.with_file_name(&name)).with_context(|| format!("Couldn't set aside {}", path.display()))?;
    for journal in ["-wal", "-shm"] {
        let _ = fs::remove_file(format!("{}{journal}", path.display()));
    }
    Ok(name)
}

/// Runs the migrations the database hasn't had yet, in one transaction. Returns the
/// version it was at before. Refuses a database from a newer version of the app.
fn migrate(conn: &mut Connection, migrations: &[&str]) -> Result<usize> {
    // IMMEDIATE takes the write lock first, so two copies of the app starting together
    // can't both run the same step.
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let version = tx.pragma_query_value(None, "user_version", |row| row.get::<_, i64>(0))?;
    let version = usize::try_from(version).unwrap_or(usize::MAX);
    if version > migrations.len() {
        bail!(
            "This data was saved by a newer version of Focus Hub (database version {version}; this \
             version knows up to {}). Update the app to open it.",
            migrations.len()
        );
    }
    if version == migrations.len() {
        return Ok(version); // up to date: write nothing, so other copies see no change
    }
    for (index, step) in migrations.iter().enumerate().skip(version) {
        tx.execute_batch(step)
            .with_context(|| format!("Couldn't upgrade the data to version {}", index + 1))?;
    }
    tx.pragma_update(None, "user_version", migrations.len() as i64)?;
    tx.commit()?;
    Ok(version)
}

fn data_version(conn: &Connection) -> Result<i64> {
    Ok(conn.pragma_query_value(None, "data_version", |row| row.get(0))?)
}

fn read_all(conn: &Connection) -> Result<AppData> {
    let mut data = AppData::default();

    let mut todos = conn.prepare("SELECT date, text, completed FROM todos ORDER BY date, position")?;
    let rows = todos.query_map([], |row| Ok((row.get::<_, NaiveDate>(0)?, row.get(1)?, row.get(2)?)))?;
    for row in rows {
        let (date, text, completed) = row?;
        data.todos_by_date
            .entry(date)
            .or_default()
            .push(TodoItem { text, completed });
    }

    let mut stats = conn.prepare("SELECT date, study_seconds, sessions FROM daily_stats")?;
    let rows = stats.query_map([], |row| {
        Ok((
            row.get::<_, NaiveDate>(0)?,
            row.get::<_, i64>(1)?,
            row.get::<_, i64>(2)?,
        ))
    })?;
    for row in rows {
        let (date, seconds, sessions) = row?;
        if seconds > 0 {
            data.stats.daily_study_seconds.insert(date, seconds as u64);
        }
        if sessions > 0 {
            data.stats.daily_streaks.insert(date, sessions as u32);
        }
    }

    let mut months = conn.prepare("SELECT month, sessions FROM monthly_sessions")?;
    let rows = months.query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)))?;
    data.stats.monthly_streaks = rows
        .map(|row| row.map(|(month, sessions)| (month, sessions as u32)))
        .collect::<rusqlite::Result<HashMap<_, _>>>()?;

    let mut rewards = conn.prepare("SELECT name, completed FROM rewards ORDER BY position")?;
    data.rewards = rewards
        .query_map([], |row| {
            Ok(Reward {
                name: row.get(0)?,
                completed: row.get(1)?,
            })
        })?
        .collect::<rusqlite::Result<_>>()?;

    if let Some((settings, background)) = conn
        .query_row(
            "SELECT work_secs, break_secs, long_break_secs, loops, long_break_every, auto_start,
                    gmt_offset_hours, animate_background, home_icon_offered, background_path
             FROM settings WHERE id = 1",
            [],
            |row| {
                let settings = Settings {
                    work_secs: row.get::<_, i64>(0)? as u64,
                    break_secs: row.get::<_, i64>(1)? as u64,
                    long_break_secs: row.get::<_, i64>(2)? as u64,
                    loops: row.get(3)?,
                    long_break_every: row.get(4)?,
                    auto_start: row.get(5)?,
                    gmt_offset_hours: row.get(6)?,
                    animate_background: row.get(7)?,
                    home_icon_offered: row.get(8)?,
                };
                Ok((settings, row.get::<_, Option<String>>(9)?))
            },
        )
        .optional()?
    {
        data.settings = settings;
        data.gif_path = background;
    }

    data.timer = conn
        .query_row(
            "SELECT mode, long_break, running, remaining_ms, current_loop, last_update_ms FROM timer WHERE id = 1",
            [],
            |row| {
                let mode: String = row.get(0)?;
                Ok(TimerSave {
                    mode: if mode == "break" {
                        TimerMode::Break
                    } else {
                        TimerMode::Work
                    },
                    long_break: row.get(1)?,
                    running: row.get(2)?,
                    remaining_ms: row.get::<_, i64>(3)? as u64,
                    current_loop: row.get(4)?,
                    last_update_ms: row.get(5)?,
                })
            },
        )
        .optional()?;
    Ok(data)
}

fn write_everything(tx: &Transaction, data: &AppData) -> Result<()> {
    tx.execute_batch("DELETE FROM todos; DELETE FROM daily_stats; DELETE FROM monthly_sessions;")?;
    for date in data.todos_by_date.keys() {
        write_todos(tx, data, *date)?;
    }
    let stats = &data.stats;
    let days: BTreeSet<NaiveDate> = stats
        .daily_study_seconds
        .keys()
        .chain(stats.daily_streaks.keys())
        .copied()
        .collect();
    for date in days {
        write_stats(tx, data, date)?;
    }
    // Months with sessions but no daily rows (possible in old desktop files).
    for (month, sessions) in &stats.monthly_streaks {
        tx.execute(
            "INSERT OR REPLACE INTO monthly_sessions (month, sessions) VALUES (?1, ?2)",
            params![month, sessions],
        )?;
    }
    write_rewards(tx, data)?;
    write_settings(tx, data)
}

fn write_todos(tx: &Transaction, data: &AppData, date: NaiveDate) -> Result<()> {
    tx.execute("DELETE FROM todos WHERE date = ?1", [date])?;
    let mut insert =
        tx.prepare_cached("INSERT INTO todos (date, position, text, completed) VALUES (?1, ?2, ?3, ?4)")?;
    for (position, todo) in data.todos_by_date.get(&date).into_iter().flatten().enumerate() {
        insert.execute(params![date, position as i64, todo.text, todo.completed])?;
    }
    Ok(())
}

fn write_stats(tx: &Transaction, data: &AppData, date: NaiveDate) -> Result<()> {
    let stats = &data.stats;
    let seconds = stats.daily_study_seconds.get(&date).copied().unwrap_or(0);
    let sessions = stats.daily_streaks.get(&date).copied().unwrap_or(0);
    if seconds == 0 && sessions == 0 {
        tx.execute("DELETE FROM daily_stats WHERE date = ?1", [date])?;
    } else {
        tx.execute(
            "INSERT INTO daily_stats (date, study_seconds, sessions) VALUES (?1, ?2, ?3)
             ON CONFLICT (date) DO UPDATE SET study_seconds = excluded.study_seconds, sessions = excluded.sessions",
            params![date, seconds as i64, sessions],
        )?;
    }
    let month = month_key(date);
    match stats.monthly_streaks.get(&month) {
        Some(sessions) => tx.execute(
            "INSERT INTO monthly_sessions (month, sessions) VALUES (?1, ?2)
             ON CONFLICT (month) DO UPDATE SET sessions = excluded.sessions",
            params![month, sessions],
        )?,
        None => tx.execute("DELETE FROM monthly_sessions WHERE month = ?1", [month])?,
    };
    Ok(())
}

fn write_rewards(tx: &Transaction, data: &AppData) -> Result<()> {
    tx.execute("DELETE FROM rewards", [])?;
    let mut insert = tx.prepare_cached("INSERT INTO rewards (position, name, completed) VALUES (?1, ?2, ?3)")?;
    for (position, reward) in data.rewards.iter().enumerate() {
        insert.execute(params![position as i64, reward.name, reward.completed])?;
    }
    Ok(())
}

fn write_settings(tx: &Transaction, data: &AppData) -> Result<()> {
    let s = &data.settings;
    tx.execute(
        "INSERT OR REPLACE INTO settings (id, work_secs, break_secs, long_break_secs, loops, long_break_every,
             auto_start, gmt_offset_hours, animate_background, home_icon_offered, background_path)
         VALUES (1, ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        params![
            s.work_secs as i64,
            s.break_secs as i64,
            s.long_break_secs as i64,
            s.loops,
            s.long_break_every,
            s.auto_start,
            s.gmt_offset_hours,
            s.animate_background,
            s.home_icon_offered,
            data.gif_path,
        ],
    )?;
    Ok(())
}

fn write_timer(tx: &Transaction, timer: Option<&TimerSave>) -> Result<()> {
    match timer {
        Some(t) => tx.execute(
            "INSERT OR REPLACE INTO timer (id, mode, long_break, running, remaining_ms, current_loop, last_update_ms)
             VALUES (1, ?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                if t.mode == TimerMode::Break { "break" } else { "work" },
                t.long_break,
                t.running,
                t.remaining_ms as i64,
                t.current_loop,
                t.last_update_ms,
            ],
        )?,
        None => tx.execute("DELETE FROM timer", [])?,
    };
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::domain::Stats;

    fn day(d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 9, d).unwrap()
    }

    fn open(dir: &Path) -> Opened {
        Database::open(dir, 1234, day(26)).unwrap()
    }

    fn user_version(path: &Path) -> i64 {
        Connection::open(path)
            .unwrap()
            .pragma_query_value(None, "user_version", |r| r.get(0))
            .unwrap()
    }

    fn sample() -> AppData {
        let mut data = AppData::default();
        data.todos_by_date.insert(
            day(25),
            vec![
                TodoItem {
                    text: "Read".into(),
                    completed: true,
                },
                TodoItem {
                    text: "Gym".into(),
                    completed: false,
                },
            ],
        );
        data.todos_by_date.insert(
            day(26),
            vec![TodoItem {
                text: "Write".into(),
                completed: false,
            }],
        );
        data.stats.daily_study_seconds.insert(day(25), 3725);
        data.stats.daily_streaks.insert(day(25), 2);
        data.stats.monthly_streaks.insert("2026-9".into(), 2);
        data.rewards = vec![Reward {
            name: "Cake".into(),
            completed: false,
        }];
        data.settings.work_secs = 1500;
        data.settings.gmt_offset_hours = Some(7);
        data.settings.animate_background = false;
        data.gif_path = Some("/data/backgrounds/duck.gif".into());
        data.timer = Some(TimerSave {
            mode: TimerMode::Break,
            running: true,
            remaining_ms: 12_345,
            current_loop: 2,
            last_update_ms: Some(1_790_000_000_000),
            long_break: true,
        });
        data
    }

    fn everything() -> BTreeSet<Change> {
        BTreeSet::from([Change::Everything])
    }

    #[test]
    fn a_new_database_gets_the_latest_schema() {
        let dir = tempfile::tempdir().unwrap();
        let opened = open(dir.path());
        assert_eq!(opened.data, AppData::default());
        assert!(opened.warning.is_none());
        assert_eq!(user_version(&dir.path().join(FILE_NAME)), MIGRATIONS.len() as i64);
    }

    #[test]
    fn everything_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let mut db = open(dir.path()).db;
        db.write(&sample(), &everything()).unwrap();
        assert_eq!(open(dir.path()).data, sample());
    }

    #[test]
    fn only_the_changed_parts_are_written() {
        let dir = tempfile::tempdir().unwrap();
        let mut db = open(dir.path()).db;
        db.write(&sample(), &everything()).unwrap();

        let mut data = sample();
        data.todos_by_date.get_mut(&day(25)).unwrap()[1].completed = true;
        data.todos_by_date.get_mut(&day(26)).unwrap()[0].text = "not saved".into();
        data.rewards.clear();
        db.write(&data, &BTreeSet::from([Change::Todos(day(25))])).unwrap();

        let saved = open(dir.path()).data;
        assert!(saved.todos_by_date[&day(25)][1].completed);
        assert_eq!(
            saved.todos_by_date[&day(26)][0].text,
            "Write",
            "day 26 wasn't part of the change"
        );
        assert_eq!(saved.rewards.len(), 1, "rewards weren't part of the change");
    }

    #[test]
    fn emptied_days_and_stats_disappear() {
        let dir = tempfile::tempdir().unwrap();
        let mut db = open(dir.path()).db;
        db.write(&sample(), &everything()).unwrap();
        let mut data = sample();
        data.todos_by_date.remove(&day(26));
        data.stats = Stats::default();
        db.write(&data, &BTreeSet::from([Change::Todos(day(26)), Change::Stats(day(25))]))
            .unwrap();
        let saved = open(dir.path()).data;
        assert!(!saved.todos_by_date.contains_key(&day(26)));
        assert_eq!(saved.stats, Stats::default());
    }

    #[test]
    fn a_write_by_the_other_copy_is_noticed() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = open(dir.path()).db;
        let mut notification = open(dir.path()).db;
        assert!(!app.changed_elsewhere().unwrap());

        app.write(&sample(), &everything()).unwrap();
        assert!(!app.changed_elsewhere().unwrap(), "its own writes don't count");
        assert!(notification.changed_elsewhere().unwrap());
        assert_eq!(notification.load().unwrap(), sample());
        assert!(
            !notification.changed_elsewhere().unwrap(),
            "until the next write elsewhere"
        );
    }

    #[test]
    fn migrations_upgrade_an_older_database_and_keep_its_data() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(FILE_NAME);
        let mut db = open(dir.path()).db;
        db.write(&sample(), &everything()).unwrap();
        drop(db);

        // A later version adds a column.
        let next = [
            MIGRATIONS[0],
            "ALTER TABLE rewards ADD COLUMN note TEXT NOT NULL DEFAULT ''",
        ];
        let mut conn = connect(&path).unwrap();
        assert_eq!(migrate(&mut conn, &next).unwrap(), 1);
        assert_eq!(
            migrate(&mut conn, &next).unwrap(),
            2,
            "already up to date: nothing runs twice"
        );
        assert_eq!(user_version(&path), 2);
        let note: String = conn.query_row("SELECT note FROM rewards", [], |r| r.get(0)).unwrap();
        assert_eq!(note, "");
        assert_eq!(read_all(&conn).unwrap().rewards, sample().rewards);
    }

    #[test]
    fn a_database_from_a_newer_version_is_refused_and_kept() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(FILE_NAME);
        drop(open(dir.path()));
        Connection::open(&path)
            .unwrap()
            .pragma_update(None, "user_version", 99)
            .unwrap();

        let error = Database::open(dir.path(), 0, day(26)).err().unwrap();
        assert!(error.to_string().contains("newer version of Focus Hub"), "{error}");
        assert_eq!(user_version(&path), 99, "left as it was");
    }

    #[test]
    fn a_failed_migration_changes_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(FILE_NAME);
        drop(open(dir.path()));
        let broken = [MIGRATIONS[0], "CREATE TABLE extra (id INTEGER); THIS IS NOT SQL"];
        let mut conn = connect(&path).unwrap();
        assert!(migrate(&mut conn, &broken).is_err());
        assert_eq!(user_version(&path), 1);
        let extra: Option<String> = conn
            .query_row("SELECT name FROM sqlite_master WHERE name = 'extra'", [], |r| r.get(0))
            .optional()
            .unwrap();
        assert!(extra.is_none(), "the half-done step was rolled back");
    }

    #[test]
    fn data_from_the_json_version_is_moved_in_once() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join(LEGACY_JSON), serde_json::to_string(&sample()).unwrap()).unwrap();

        let opened = open(dir.path());
        assert!(opened.warning.is_none());
        assert_eq!(opened.data, sample());
        assert!(!dir.path().join(LEGACY_JSON).exists());
        assert!(
            dir.path().join(LEGACY_JSON_MOVED).exists(),
            "the old file is kept, renamed"
        );

        let mut db = opened.db;
        db.write(&AppData::default(), &everything()).unwrap();
        fs::copy(dir.path().join(LEGACY_JSON_MOVED), dir.path().join(LEGACY_JSON)).unwrap();
        assert_eq!(
            open(dir.path()).data,
            AppData::default(),
            "only a new database takes it in"
        );
    }

    #[test]
    fn a_damaged_json_file_is_kept_aside() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join(LEGACY_JSON), "{ this is not json").unwrap();
        let opened = open(dir.path());
        assert_eq!(opened.data, AppData::default());
        assert!(opened.warning.unwrap().contains("focushub_data.damaged-1234.json"));
        let kept = fs::read_to_string(dir.path().join("focushub_data.damaged-1234.json")).unwrap();
        assert_eq!(kept, "{ this is not json");
    }

    #[test]
    fn a_damaged_database_is_kept_aside() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join(FILE_NAME),
            "definitely not a database, but long enough to look like one?",
        )
        .unwrap();
        let opened = open(dir.path());
        assert_eq!(opened.data, AppData::default());
        assert!(opened.warning.unwrap().contains("focushub.damaged-1234.db"));
        assert!(dir.path().join("focushub.damaged-1234.db").exists());
        assert_eq!(
            user_version(&dir.path().join(FILE_NAME)),
            MIGRATIONS.len() as i64,
            "a fresh one replaced it"
        );
    }

    #[test]
    fn reads_the_desktop_apps_json() {
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
        let data = parse_json(json).unwrap();
        let date = NaiveDate::from_ymd_opt(2025, 7, 25).unwrap();
        assert_eq!(data.todos_by_date[&date][0].text, "Study Rust");
        assert_eq!(data.stats.daily_study_seconds[&date], 3600);
        assert_eq!(data.stats.monthly_streaks["2025-7"], 5);
        assert_eq!(data.rewards[0].name, "Ice cream");
        assert_eq!(data.settings.work_secs, 3600, "missing settings use the defaults");
        assert!(data.settings.auto_start && data.settings.animate_background);
        assert!(data.timer.is_none());
    }

    #[test]
    fn reads_the_first_mobile_versions_json() {
        // Settings without the fields added later.
        let json = r#"{ "todos_by_date": {}, "stats": {}, "rewards": [],
            "settings": { "work_secs": 1500, "break_secs": 300, "loops": 4, "gmt_offset_hours": 7 } }"#;
        let data = parse_json(json).unwrap();
        assert_eq!(data.settings.work_secs, 1500);
        assert_eq!(data.settings.gmt_offset_hours, Some(7));
        assert_eq!(data.settings.long_break_secs, 15 * 60);
        assert!(data.settings.auto_start);
    }

    #[test]
    fn daily_backups_keep_the_newest_seven_and_can_be_read() {
        let dir = tempfile::tempdir().unwrap();
        let mut db = open(dir.path()).db;
        assert!(db.backups().is_empty(), "a brand-new database isn't backed up");

        let mut data = AppData::default();
        for d in 1..=10 {
            data.rewards = vec![Reward {
                name: format!("day {d}"),
                completed: false,
            }];
            db.write(&data, &BTreeSet::from([Change::Rewards])).unwrap();
            db.back_up_daily(day(d)).unwrap();
            db.back_up_daily(day(d)).unwrap(); // twice a day: still one backup
        }
        assert_eq!(db.backups(), (4..=10).rev().map(day).collect::<Vec<_>>());
        assert_eq!(db.read_backup(day(4)).unwrap().rewards[0].name, "day 4");
        assert!(db.read_backup(day(1)).is_err());
    }
}
