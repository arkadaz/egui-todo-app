use super::*;

const SEC: i64 = 1000;

fn day(d: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 9, d).unwrap()
}

fn open(dir: &Path) -> Hub {
    Hub::open(dir, 0, day(26)).unwrap()
}

fn settings(work_secs: u64, break_secs: u64, loops: u32) -> TimerSettings {
    TimerSettings {
        work_secs,
        break_secs,
        long_break_secs: break_secs * 3,
        loops,
        long_break_every: 0,
        auto_start: true,
    }
}

fn texts(todos: &[TodoItem]) -> Vec<&str> {
    todos.iter().map(|t| t.text.as_str()).collect()
}

#[test]
fn todos_add_toggle_edit_delete_and_persist() {
    let dir = tempfile::tempdir().unwrap();
    let mut hub = open(dir.path());
    hub.add_todo(day(26), "  Read chapter 3  ", 0).unwrap();
    hub.add_todo(day(26), "Practice", 0).unwrap();
    assert!(hub.add_todo(day(26), "   ", 0).is_err());
    assert!(hub.add_todo(day(26), &"x".repeat(201), 0).is_err());
    hub.set_todo_completed(day(26), 0, true, 0).unwrap();
    hub.edit_todo(day(26), 0, " Read chapter 4 ", 0).unwrap();
    assert!(hub.edit_todo(day(26), 0, "", 0).is_err());
    assert!(hub.edit_todo(day(26), 9, "x", 0).is_err());
    let removed = hub.delete_todo(day(26), 1, 0).unwrap();
    assert_eq!(removed.text, "Practice");
    assert!(hub.delete_todo(day(26), 5, 0).is_err());

    let reopened = open(dir.path());
    assert_eq!(
        reopened.todos(day(26)),
        vec![TodoItem {
            text: "Read chapter 4".into(),
            completed: true
        }]
    );
}

#[test]
fn undo_puts_a_deleted_task_back_in_place() {
    let dir = tempfile::tempdir().unwrap();
    let mut hub = open(dir.path());
    for t in ["A", "B", "C"] {
        hub.add_todo(day(26), t, 0).unwrap();
    }
    let removed = hub.delete_todo(day(26), 1, 0).unwrap();
    hub.restore_todo(day(26), 1, removed, 0).unwrap();
    assert_eq!(texts(&hub.todos(day(26))), vec!["A", "B", "C"]);

    // Restoring the only task of a day brings the day back.
    hub.add_todo(day(20), "Only", 0).unwrap();
    let only = hub.delete_todo(day(20), 0, 0).unwrap();
    assert_eq!(hub.task_marks(day(1), day(30)).len(), 1);
    hub.restore_todo(day(20), 5, only, 0).unwrap();
    assert_eq!(texts(&hub.todos(day(20))), vec!["Only"]);
}

#[test]
fn tasks_can_be_reordered() {
    let dir = tempfile::tempdir().unwrap();
    let mut hub = open(dir.path());
    for t in ["A", "B", "C", "D"] {
        hub.add_todo(day(26), t, 0).unwrap();
    }
    hub.move_todo(day(26), 0, 2, 0).unwrap();
    assert_eq!(texts(&hub.todos(day(26))), vec!["B", "C", "A", "D"]);
    hub.move_todo(day(26), 3, 0, 0).unwrap();
    assert_eq!(texts(&hub.todos(day(26))), vec!["D", "B", "C", "A"]);
    hub.move_todo(day(26), 1, 99, 0).unwrap();
    assert_eq!(texts(&hub.todos(day(26))), vec!["D", "C", "A", "B"]);
    assert!(hub.move_todo(day(26), 9, 0, 0).is_err());
}

#[test]
fn deleting_the_last_task_removes_the_day() {
    let dir = tempfile::tempdir().unwrap();
    let mut hub = open(dir.path());
    hub.add_todo(day(26), "Only one", 0).unwrap();
    hub.delete_todo(day(26), 0, 0).unwrap();
    assert!(hub.task_marks(day(1), day(30)).is_empty());
}

#[test]
fn history_lists_earlier_days_newest_first() {
    let dir = tempfile::tempdir().unwrap();
    let mut hub = open(dir.path());
    hub.add_todo(day(20), "A", 0).unwrap();
    hub.add_todo(day(24), "B", 0).unwrap();
    hub.add_todo(day(26), "Today", 0).unwrap();
    hub.add_todo(day(28), "Later", 0).unwrap();
    let days: Vec<_> = hub
        .history(day(26), HistoryFilter::All, "")
        .into_iter()
        .map(|d| d.date)
        .collect();
    assert_eq!(days, vec![day(24), day(20)]);
    let marks = hub.task_marks(day(1), day(30));
    assert_eq!(marks.len(), 4);
    assert!(marks.values().all(|unfinished| *unfinished));
    assert!(hub.task_marks(day(29), day(30)).is_empty());
}

#[test]
fn history_can_be_filtered_and_searched() {
    let dir = tempfile::tempdir().unwrap();
    let mut hub = open(dir.path());
    hub.add_todo(day(20), "Read Rust book", 0).unwrap();
    hub.add_todo(day(20), "Gym", 0).unwrap();
    hub.add_todo(day(20), "rust exercises", 0).unwrap();
    hub.set_todo_completed(day(20), 0, true, 0).unwrap();
    hub.add_todo(day(24), "Groceries", 0).unwrap();
    hub.set_todo_completed(day(24), 0, true, 0).unwrap();

    let unfinished = hub.history(day(26), HistoryFilter::Unfinished, "");
    assert_eq!(unfinished.len(), 1, "day 24 has nothing unfinished");
    assert_eq!(
        unfinished[0].todos.iter().map(|(i, _)| *i).collect::<Vec<_>>(),
        vec![1, 2]
    );
    assert_eq!(
        (unfinished[0].done, unfinished[0].total),
        (1, 3),
        "counts cover the whole day"
    );

    let done = hub.history(day(26), HistoryFilter::Done, "");
    assert_eq!(done.iter().map(|d| d.date).collect::<Vec<_>>(), vec![day(24), day(20)]);

    let rust = hub.history(day(26), HistoryFilter::All, "  RUST ");
    assert_eq!(rust.len(), 1);
    let found: Vec<_> = rust[0].todos.iter().map(|(i, t)| (*i, t.text.as_str())).collect();
    assert_eq!(
        found,
        vec![(0, "Read Rust book"), (2, "rust exercises")],
        "ignores case, keeps positions"
    );
    assert!(hub.history(day(26), HistoryFilter::Unfinished, "groceries").is_empty());
}

#[test]
fn calendar_marks_show_whether_a_day_is_finished() {
    let dir = tempfile::tempdir().unwrap();
    let mut hub = open(dir.path());
    hub.add_todo(day(20), "Done", 0).unwrap();
    hub.set_todo_completed(day(20), 0, true, 0).unwrap();
    hub.add_todo(day(21), "Not yet", 0).unwrap();
    let marks = hub.task_marks(day(20), day(21));
    assert_eq!(marks, HashMap::from([(day(20), false), (day(21), true)]));
}

#[test]
fn unfinished_tasks_move_to_today() {
    let dir = tempfile::tempdir().unwrap();
    let mut hub = open(dir.path());
    hub.add_todo(day(20), "Old unfinished", 0).unwrap();
    hub.add_todo(day(20), "Old done", 0).unwrap();
    hub.set_todo_completed(day(20), 1, true, 0).unwrap();
    hub.add_todo(day(24), "Newer unfinished", 0).unwrap();
    hub.add_todo(day(26), "Today's", 0).unwrap();
    hub.add_todo(day(28), "Future", 0).unwrap();
    assert_eq!(hub.unfinished_before(day(26)), 2);

    assert_eq!(hub.move_unfinished_to(day(26), 0).unwrap(), 2);
    assert_eq!(
        texts(&hub.todos(day(26))),
        vec!["Today's", "Old unfinished", "Newer unfinished"]
    );
    assert_eq!(texts(&hub.todos(day(20))), vec!["Old done"], "finished tasks stay");
    assert!(hub.todos(day(24)).is_empty(), "emptied days disappear");
    assert_eq!(texts(&hub.todos(day(28))), vec!["Future"], "later days aren't touched");
    assert_eq!(hub.unfinished_before(day(26)), 0);
    assert_eq!(hub.move_unfinished_to(day(26), 0).unwrap(), 0);
}

#[test]
fn rewards_keep_unfinished_first_and_can_be_edited_and_restored() {
    let dir = tempfile::tempdir().unwrap();
    let mut hub = open(dir.path());
    hub.add_reward("Cake", 0).unwrap();
    hub.add_reward("Movie", 0).unwrap();
    hub.set_reward_completed(0, true, 0).unwrap(); // Cake done
    let names = |hub: &Hub| hub.rewards().iter().map(|r| r.name.clone()).collect::<Vec<_>>();
    assert_eq!(names(&hub), vec!["Movie", "Cake"]);
    hub.edit_reward(0, "Movie night", 0).unwrap();
    let removed = hub.delete_reward(1, 0).unwrap();
    assert_eq!(names(&hub), vec!["Movie night"]);
    hub.restore_reward(0, removed, 0).unwrap();
    assert_eq!(names(&hub), vec!["Movie night", "Cake"], "sorted again after restoring");
    assert!(hub.set_reward_completed(9, true, 0).is_err());
    assert!(hub.edit_reward(9, "x", 0).is_err());
}

#[test]
fn timer_records_study_time_and_sessions() {
    let dir = tempfile::tempdir().unwrap();
    let mut hub = open(dir.path());
    hub.set_timer_settings(settings(10, 5, 1), 0, day(26)).unwrap();
    hub.toggle_timer(0, day(26)).unwrap();
    assert!(hub.tick(4 * SEC + 500, day(26)).unwrap().ended.is_empty());
    assert_eq!(hub.study_seconds_on(day(26)), 4);

    let ended = hub.tick(15 * SEC, day(26)).unwrap().ended;
    let modes: Vec<_> = ended.iter().map(|c| c.mode).collect();
    assert_eq!(modes, vec![TimerMode::Work, TimerMode::Break]);
    assert_eq!(hub.study_seconds_on(day(26)), 10);
    assert_eq!(hub.sessions_on(day(26)), 1);
    assert_eq!(hub.sessions_in_month_of(day(26)), 1);
    assert!(!hub.timer().is_running());
}

#[test]
fn skipping_counts_the_time_spent_and_the_session() {
    let dir = tempfile::tempdir().unwrap();
    let mut hub = open(dir.path());
    hub.set_timer_settings(settings(100, 50, 1), 0, day(26)).unwrap();
    hub.toggle_timer(0, day(26)).unwrap();
    hub.skip_session(20 * SEC + 600, day(26)).unwrap(); // skip work after 20.6s
    assert_eq!(hub.study_seconds_on(day(26)), 21);
    assert_eq!(hub.timer().timer_mode, TimerMode::Break);
    let ended = hub.skip_session(21 * SEC, day(26)).unwrap(); // skip the break
    assert!(ended[0].all_done);
    assert_eq!(
        hub.sessions_on(day(26)),
        1,
        "a skipped break still completes the session"
    );
}

#[test]
fn a_running_timer_survives_closing_the_app() {
    let dir = tempfile::tempdir().unwrap();
    {
        let mut hub = open(dir.path());
        hub.set_timer_settings(settings(60, 30, 1), 0, day(26)).unwrap();
        hub.toggle_timer(0, day(26)).unwrap();
        hub.tick(10 * SEC, day(26)).unwrap();
    } // app closed 10s into the work session

    // Reopened 70s later: work finished (50s more study), break is 20s in.
    let mut hub = Hub::open(dir.path(), 80 * SEC, day(26)).unwrap();
    assert!(hub.timer().is_running());
    assert_eq!(hub.timer().timer_mode, TimerMode::Break);
    assert_eq!(hub.study_seconds_on(day(26)), 60);
    let ended = hub.tick(80 * SEC, day(26)).unwrap().ended;
    assert_eq!(ended.len(), 1, "the work session that ended while closed is reported");
    assert!(hub.tick(81 * SEC, day(26)).unwrap().ended.is_empty(), "...only once");
}

#[test]
fn changes_from_another_copy_of_the_app_are_picked_up() {
    let dir = tempfile::tempdir().unwrap();
    let mut main = open(dir.path());
    main.set_timer_settings(settings(100, 50, 1), 0, day(26)).unwrap();
    main.toggle_timer(0, day(26)).unwrap();
    main.add_todo(day(26), "From main", SEC).unwrap();

    // A notification button pauses the timer from a second copy of the app.
    std::thread::sleep(std::time::Duration::from_millis(20)); // a different file time
    let mut notification = Hub::open(dir.path(), 5 * SEC, day(26)).unwrap();
    notification.toggle_timer(5 * SEC, day(26)).unwrap();
    assert!(!notification.timer().is_running());

    // The main copy sees it on its next tick, and doesn't undo it.
    assert!(main.tick(6 * SEC, day(26)).unwrap().reloaded);
    assert!(!main.timer().is_running(), "the pause from the notification wins");
    assert!(
        !main.tick(6 * SEC + 250, day(26)).unwrap().reloaded,
        "only reloads once"
    );
    main.add_todo(day(26), "Also from main", 7 * SEC).unwrap();
    let reopened = open(dir.path());
    assert!(!reopened.timer().is_running());
    assert_eq!(texts(&reopened.todos(day(26))), vec!["From main", "Also from main"]);
    assert_eq!(reopened.study_seconds_on(day(26)), 5);
}

#[test]
fn hiding_the_app_does_not_undo_a_notification_button() {
    let dir = tempfile::tempdir().unwrap();
    let mut main = open(dir.path());
    main.set_timer_settings(settings(100, 50, 1), 0, day(26)).unwrap();
    main.toggle_timer(0, day(26)).unwrap();
    main.save_unless_changed(SEC).unwrap(); // the app is hidden

    std::thread::sleep(std::time::Duration::from_millis(20)); // a different file time
    let mut notification = Hub::open(dir.path(), 5 * SEC, day(26)).unwrap();
    notification.toggle_timer(5 * SEC, day(26)).unwrap(); // "Pause" in the notification

    // Coming back, the app passes through "hidden" again, which saves.
    assert!(
        main.save_unless_changed(9 * SEC).unwrap(),
        "it loads the newer file instead"
    );
    assert!(!main.timer().is_running());
    assert!(!open(dir.path()).timer().is_running(), "the pause is still saved");
}

#[test]
fn settings_are_clamped_and_saved() {
    let dir = tempfile::tempdir().unwrap();
    let mut hub = open(dir.path());
    let huge = TimerSettings {
        work_secs: 99_999,
        break_secs: 99_999,
        long_break_secs: 99_999,
        loops: 99,
        long_break_every: 99,
        auto_start: false,
    };
    hub.set_timer_settings(huge, 0, day(26)).unwrap();
    let reopened = open(dir.path());
    let s = reopened.timer_settings();
    assert_eq!(s.work_secs, MAX_WORK_SECS);
    assert_eq!(s.break_secs, MAX_BREAK_SECS);
    assert_eq!(s.long_break_secs, MAX_BREAK_SECS);
    assert_eq!(s.loops, MAX_LOOPS);
    assert_eq!(s.long_break_every, MAX_LOOPS);
    assert!(!s.auto_start);
    assert!(!reopened.timer().config.auto_start);

    hub.set_timer_settings(settings(10, 5, 0), 0, day(26)).unwrap();
    assert_eq!(hub.timer().config.total_loops, 1);
}

#[test]
fn presets_apply_and_are_recognised() {
    let dir = tempfile::tempdir().unwrap();
    let mut hub = open(dir.path());
    assert_eq!(
        hub.timer_settings().preset_index(),
        Some(2),
        "the defaults are the desktop preset"
    );
    hub.apply_preset(0, 0, day(26)).unwrap();
    let s = hub.timer_settings();
    assert_eq!(
        (s.work_secs, s.break_secs, s.loops, s.long_break_every),
        (1500, 300, 4, 4)
    );
    assert_eq!(s.preset_index(), Some(0));
    assert!(hub.apply_preset(7, 0, day(26)).is_err());
    hub.set_timer_settings(settings(10, 5, 1), 0, day(26)).unwrap();
    assert_eq!(hub.timer_settings().preset_index(), None);
}

#[test]
fn reset_keeps_the_study_time_so_far() {
    let dir = tempfile::tempdir().unwrap();
    let mut hub = open(dir.path());
    hub.set_timer_settings(settings(100, 5, 1), 0, day(26)).unwrap();
    hub.toggle_timer(0, day(26)).unwrap();
    hub.reset_timer(7 * SEC + 600, day(26)).unwrap();
    assert_eq!(hub.study_seconds_on(day(26)), 8);
    assert!(hub.timer().is_at_start());
}

#[test]
fn streaks_and_best_day() {
    let dir = tempfile::tempdir().unwrap();
    let mut hub = open(dir.path());
    let study = &mut hub.data.stats.daily_study_seconds;
    for (d, secs) in [
        (10, 600),
        (11, 60),
        (12, 1800),
        (20, 300),
        (21, 30),
        (24, 900),
        (25, 120),
    ] {
        study.insert(day(d), secs);
    }
    // 21st has only 30s: below the 60s minimum, so it breaks the run.
    assert_eq!(hub.best_streak(), 3);
    assert_eq!(hub.current_streak(day(25)), 2, "24th and 25th");
    assert_eq!(
        hub.current_streak(day(26)),
        2,
        "today not studied yet: streak still alive"
    );
    assert_eq!(hub.current_streak(day(27)), 0, "a missed day ends it");
    assert_eq!(hub.best_day(), Some((day(12), 1800)));

    let series = hub.study_series(day(12), 4);
    assert_eq!(
        series,
        vec![(day(9), 0), (day(10), 600), (day(11), 60), (day(12), 1800)]
    );
}

#[test]
fn time_zone_setting_is_validated() {
    let dir = tempfile::tempdir().unwrap();
    let mut hub = open(dir.path());
    hub.set_gmt_offset_hours(Some(7), 0).unwrap();
    assert!(hub.set_gmt_offset_hours(Some(15), 0).is_err());
    assert_eq!(open(dir.path()).gmt_offset_hours(), Some(7));
    hub.set_gmt_offset_hours(None, 0).unwrap();
    assert_eq!(open(dir.path()).gmt_offset_hours(), None);
}

#[test]
fn backgrounds_are_copied_and_replaced() {
    let dir = tempfile::tempdir().unwrap();
    let mut hub = open(dir.path());
    assert!(hub.set_background("notes.txt", b"hi", 1).is_err());
    let first = hub.set_background("Duck.GIF", b"GIF89a...", 1).unwrap();
    assert_eq!(hub.background_path(), Some(first.clone()));
    let second = hub.set_background("cat.png", b"PNG...", 2).unwrap();
    assert!(!Path::new(&first).exists(), "the old copy is deleted");
    assert_eq!(open(dir.path()).background_path(), Some(second.clone()));
    hub.clear_background(3).unwrap();
    assert!(hub.background_path().is_none());
    assert!(!Path::new(&second).exists());
}

#[test]
fn a_missing_background_file_falls_back() {
    let dir = tempfile::tempdir().unwrap();
    let mut hub = open(dir.path());
    hub.data.gif_path = Some("C:\\Users\\someone\\duck.gif".into());
    assert!(hub.background_path().is_none());
    // Clearing a path we don't own must not try to delete it.
    hub.clear_background(0).unwrap();
}

const DESKTOP_JSON: &str = r#"{
    "todos_by_date": {
        "2025-07-25": [ { "text": "A", "completed": false }, { "text": "B", "completed": true } ],
        "2025-07-26": []
    },
    "stats": { "daily_study_seconds": { "2025-07-25": 100 }, "daily_streaks": {}, "monthly_streaks": {} },
    "rewards": [ { "name": "Done", "completed": true }, { "name": "Open", "completed": false } ]
}"#;

#[test]
fn import_replaces_data_but_keeps_settings() {
    let dir = tempfile::tempdir().unwrap();
    let mut hub = open(dir.path());
    hub.set_timer_settings(settings(25 * 60, 5 * 60, 4), 0, day(26))
        .unwrap();
    hub.add_todo(day(26), "Old task", 0).unwrap();
    let imported = hub.import_json(DESKTOP_JSON, 0).unwrap();
    assert_eq!((imported.days, imported.tasks, imported.rewards), (1, 2, 2));
    assert!(hub.todos(day(26)).is_empty());
    assert_eq!(hub.rewards()[0].name, "Open", "rewards are sorted after import");
    assert_eq!(hub.timer().config.total_loops, 4, "settings are kept");
    assert!(hub.import_json("not json", 0).is_err());
}

#[test]
fn merge_combines_without_counting_twice() {
    let dir = tempfile::tempdir().unwrap();
    let mut hub = open(dir.path());
    let july25 = NaiveDate::from_ymd_opt(2025, 7, 25).unwrap();
    hub.add_todo(july25, "A", 0).unwrap(); // same text as in the file
    hub.add_todo(july25, "Phone only", 0).unwrap();
    hub.data.stats.daily_study_seconds.insert(july25, 40);
    hub.data.stats.daily_study_seconds.insert(day(26), 500);
    hub.add_reward("Open", 0).unwrap();
    hub.add_reward("Phone reward", 0).unwrap();

    let merged = hub.merge_json(DESKTOP_JSON, 0).unwrap();
    assert_eq!(
        (merged.tasks_added, merged.study_days_updated, merged.rewards_added),
        (1, 1, 1)
    );
    assert_eq!(texts(&hub.todos(july25)), vec!["A", "Phone only", "B"]);
    assert_eq!(hub.study_seconds_on(july25), 100, "the larger number wins");
    assert_eq!(hub.study_seconds_on(day(26)), 500, "our own days stay");
    let names: Vec<_> = hub.rewards().iter().map(|r| r.name.as_str()).collect();
    assert_eq!(names, vec!["Open", "Phone reward", "Done"]);

    // Merging the same file again changes nothing.
    let again = hub.merge_json(DESKTOP_JSON, 0).unwrap();
    assert_eq!(
        (again.tasks_added, again.study_days_updated, again.rewards_added),
        (0, 0, 0)
    );
    assert_eq!(hub.study_seconds_on(july25), 100);
    assert!(hub.merge_json("{", 0).is_err());
}

#[test]
fn a_task_finished_in_either_copy_stays_finished() {
    let dir = tempfile::tempdir().unwrap();
    let mut hub = open(dir.path());
    let july25 = NaiveDate::from_ymd_opt(2025, 7, 25).unwrap();
    hub.add_todo(july25, "B", 0).unwrap(); // unfinished here, finished in the file
    hub.merge_json(DESKTOP_JSON, 0).unwrap();
    assert!(hub.todos(july25).iter().find(|t| t.text == "B").unwrap().completed);
}

#[test]
fn export_round_trips_through_import() {
    let dir = tempfile::tempdir().unwrap();
    let mut hub = open(dir.path());
    hub.add_todo(day(26), "Keep me", 0).unwrap();
    hub.add_reward("Prize", 0).unwrap();
    let json = hub.export_json().unwrap();

    let other = tempfile::tempdir().unwrap();
    let mut hub2 = open(other.path());
    hub2.import_json(&json, 0).unwrap();
    assert_eq!(hub2.todos(day(26))[0].text, "Keep me");
    assert_eq!(hub2.rewards()[0].name, "Prize");
}

#[test]
fn daily_backups_can_be_restored() {
    let dir = tempfile::tempdir().unwrap();
    {
        let mut hub = Hub::open(dir.path(), 0, day(25)).unwrap();
        hub.add_todo(day(25), "Before the mistake", 0).unwrap();
        hub.set_timer_settings(settings(1500, 300, 4), 0, day(25)).unwrap();
    }
    // The first open had no file to back up yet. Opening on the 26th backs up the data
    // as it was at the end of the 25th.
    let mut hub = Hub::open(dir.path(), 0, day(26)).unwrap();
    hub.delete_todo(day(25), 0, 0).unwrap();
    hub.set_timer_settings(settings(10, 5, 1), 0, day(26)).unwrap();
    assert_eq!(hub.backups(), vec![day(26)]);

    hub.restore_backup(day(26), 0).unwrap();
    assert_eq!(texts(&hub.todos(day(25))), vec!["Before the mistake"]);
    assert_eq!(hub.timer_settings().loops, 4, "settings come back too");
    assert!(hub.restore_backup(day(1), 0).is_err());
}

#[test]
fn a_damaged_database_opens_with_a_warning() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join(database::FILE_NAME), "oops").unwrap();
    let hub = open(dir.path());
    assert!(hub.load_warning().is_some());
}
