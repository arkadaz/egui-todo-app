# Focus Hub (Flutter + Rust)

A Pomodoro timer over an animated background, to-dos per day with a calendar and
history, study stats, and rewards, for **Android and iOS**. It started as a desktop app
written with egui; that version is kept under the git tag `desktop-egui`, and its
`focushub_data.json` still opens here.

**Flutter draws the UI. Rust does everything else**: the data, saving, the timer,
stats, dates, the calendar, the task history's filtering and search, and shrinking
background images. They talk through
[flutter_rust_bridge](https://cjycode.com/flutter_rust_bridge/).

## Features

- **Timer**: presets (Classic Pomodoro, Deep work, Desktop default), a long break
  every N loops, skip, and an "auto-start the next session" switch.
- **Alerts** when a session ends, on time even if the app is closed.
- **Live notification** (Android) while the timer runs: a countdown drawn by Android
  itself, with Pause / Skip (Resume / Reset when paused) buttons that work even if the
  app was closed. iOS has no ongoing notifications, so there it's the alerts only.
- **Tasks**: tap to edit, drag to reorder, delete with Undo, move unfinished tasks
  from earlier days to today. The calendar folds to one week; dots show which days
  still have unfinished tasks. The history is grouped by month, with All / Unfinished
  / Done filters and a search; finished days fold to one line.
- **Stats**: streak, best day, a 7- or 30-day study chart.
- **Rewards**: rename, delete with Undo.
- **Data**: merge or replace with the desktop app's `focushub_data.json`, export,
  and a daily backup of the last 7 days you can restore.
- Light and dark theme (follows the phone); a side rail on tablets.

## Layout

```
focus_hub/
├── rust/src/
│   ├── engine/            ← all the logic, plain Rust (no Flutter), with 75 tests
│   │   ├── domain.rs        the data (and the desktop app's JSON format)
│   │   ├── database.rs      SQLite: schema migrations, saving, daily backups
│   │   ├── hub/             the app's operations: timer, tasks, rewards, stats,
│   │   │                    background, import/export (one file each)
│   │   ├── timer.rs         Pomodoro timer (wall-clock based, catches up after sleep)
│   │   ├── images.rs        shrinks a chosen background to the screen (GIF frames too)
│   │   └── dates.rs         date math and formatting
│   └── api/focus_hub.rs   ← the bridge: what Dart can call
├── lib/
│   ├── main.dart            startup
│   ├── app.dart             theme, tabs, app lifecycle, first-launch Home screen offer
│   ├── controller.dart      connects the screens to Rust
│   ├── services/            notifications, Battery Saver, Home screen icon
│   ├── pages/               Focus, Tasks, Stats, Rewards
│   └── widgets/             calendar, settings sheet, dialogs
├── android/.../MainActivity.kt   Battery Saver signal and the Home screen icon request
├── ios/Runner/AppDelegate.swift  Low Power Mode signal, alerts while the app is open
├── test/                    12 fast tests on this computer (widgets, and Rust through Dart)
├── integration_test/        18 end-to-end tests on a phone/emulator, plus perf_test.dart
└── assets/                  background GIF and app icon (from the desktop app)
```

## Run it

```
flutter run                        # on a connected phone or the emulator
```

Checks, from quickest to slowest:

```
cd rust && cargo clippy --all-targets && cargo test   # Rust: lints and 75 tests
flutter analyze                                       # Dart: strict analysis
(cd rust && cargo build --release) && flutter test    # 12 tests on this computer, with the real Rust library
flutter test integration_test                         # 18 tests on a phone or emulator
```

Formatting: `cargo fmt` in `rust/` and `dart format lib test integration_test`
(both 120 columns wide, set in `rust/rustfmt.toml` and `analysis_options.yaml`).

After changing anything in `rust/src/api/`, regenerate the bridge:

```
flutter_rust_bridge_codegen generate
```

`flutter run` makes a **debug** build, which is much slower than what users get (Dart
runs without compiling ahead). Judge speed with `flutter run --profile` or `--release`.

## Build for a phone

**Android**:

```
flutter build apk --release --split-per-abi
```

Install `build/app/outputs/flutter-apk/app-arm64-v8a-release.apk` on the phone.

The app ID is `com.arkadaz.focushub`, and release builds are signed with your own key:
`android/key.properties` (never committed) points to the keystore in
`C:\Users\User\.android\focushub-release.jks`. **Keep a copy of both somewhere safe.**
Android only installs an update over the app if it's signed with the same key; with a
lost key, the only way is to uninstall (export your data first). Without
`key.properties`, release builds fall back to the debug key.

**iOS** needs a Mac with Xcode (Apple allows iOS builds nowhere else) and Rust
(`rustup`; the build adds the iOS targets by itself). Open `ios/Runner.xcworkspace`,
choose your team under *Signing & Capabilities*, then `flutter run --release` with the
iPhone connected.

## Speed and battery

The app does nothing in the background: no ticking, no drawing. Android counts down in
the notification and fires the scheduled alerts. In the foreground:

- **Ticks come right after a number on screen changes** (Rust says when), so about once
  a second instead of on a fixed fast beat, and only the clock, timer and stats redraw.
- **Hidden tabs sleep.** Tabs stay alive (so scroll positions and typing survive), but a
  hidden tab doesn't redraw, and the background GIF stops when the Focus tab is hidden.
- **The GIF also stops** in Battery Saver / Low Power Mode, when the system asks for less
  motion, or with Settings → *Animated background* off. It's the only continuous animation.
- **Images are shrunk once, in Rust**, when chosen: photos to the screen size (turned
  upright), GIFs and animated WebP to at most 1280 px, every frame, processed on all CPU
  cores. The app never decodes more pixels than the screen shows.
- Long histories are built lazily (only what's on screen).

Measured on the emulator (profile build, timer running, frames drawn in 3 idle seconds):

| Tab | Before | After |
|---|---|---|
| Focus (GIF playing) | 64 | 64 (4 with the GIF stopped) |
| Tasks | 64 | 1 |
| Stats | 64 | 5 |
| Rewards | 57 | 1 |

`integration_test/perf_test.dart` records frame build and raster times for a heavy user
(120 days of history): run it with
`flutter drive --profile --no-dds --driver=test_driver/integration_test.dart --target=integration_test/perf_test.dart`.

## Notes

- **Your desktop data**: Settings → *Import from the desktop app…* merges in (or
  replaces with) a `focushub_data.json` from the egui app, and *Export a backup…* writes
  one the desktop app can open.
- **Alerts** are scheduled with Android's exact alarms when the timer starts, so
  they arrive on time even if the phone sleeps or the app is closed. The app uses
  `USE_EXACT_ALARM` (granted automatically); the Play Store only allows that
  permission for alarm, timer and calendar apps.
- **Data** lives in a SQLite database, `focushub.db`, in the app's private folder. The
  app keeps it in memory and after each change writes only what changed, in one
  transaction. The tables come from numbered migrations in `database.rs` (SQLite's
  `user_version` says which have run): to change the schema, add a step at the end,
  never edit a shipped one. A failed step rolls back; a database from a newer app version
  is refused, not touched. On first run, an old `focushub_data.json` is moved in (and
  kept as `focushub_data.before-database.json`). Daily backups are `VACUUM INTO` copies
  in `backups/`.
- **Notification buttons** run in a second, short-lived copy of the app with its own
  database connection; before every change (and before saving when the app is
  hidden) Rust asks SQLite whether the other connection wrote (`PRAGMA data_version`) and
  reloads if so.
- **Home screen icon**: iOS always adds it. Android doesn't let apps add their own icon
  silently: on first launch the app offers it (Android then asks you to confirm), and
  Settings has it too. Launchers such as Pixel's also add new apps by themselves when
  their "Add app icons to Home screen" option is on.
- **Uninstalling removes all data**: everything is in the app's private folder. On
  Android cloud backup is off (`allowBackup="false"` and `data_extraction_rules.xml`), so
  a reinstall starts fresh. Export a backup first if you want to keep it.
- `android/gradle.properties` turns off Kotlin incremental compilation: the project
  (D:) and Flutter's package cache (C:) are on different drives, which breaks it.
