# Focus Hub (Flutter + Rust)

A phone version of the desktop Focus Hub (`C:\Code\Rust\alarm\egui-todo-app`):
a Pomodoro timer over an animated background, to-dos per day with a calendar and
history, study stats, and rewards.

**Flutter draws the UI. Rust does everything else**: the data, saving, the timer,
stats, dates, the calendar, the task history's filtering and search, and shrinking
background images. They talk through
[flutter_rust_bridge](https://cjycode.com/flutter_rust_bridge/).

## Features

- **Timer**: presets (Classic Pomodoro, Deep work, Desktop default), a long break
  every N loops, skip, and an "auto-start the next session" switch.
- **Live notification** while the timer runs: a countdown drawn by Android itself,
  with Pause / Skip (Resume / Reset when paused) buttons that work even if the app
  was closed.
- **Tasks**: tap to edit, drag to reorder, delete with Undo, move unfinished tasks
  from earlier days to today. The calendar folds to one week; dots show which days
  still have unfinished tasks. The history is grouped by month, with All / Unfinished
  / Done filters and a search; finished days fold to one line.
- **Stats**: streak, best day, a 7- or 30-day study chart.
- **Rewards**: rename, delete with Undo.
- **Data**: merge or replace with the desktop app's `focushub_data.json`, export,
  and a daily backup of the last 7 days you can restore.
- Light and dark theme (follows the phone); a side rail on wide screens.

## Layout

```
focus_hub/
├── rust/src/
│   ├── engine/            ← all the logic, plain Rust (no Flutter), with 67 tests
│   │   ├── domain.rs        saved data (same JSON as the desktop app)
│   │   ├── timer.rs         Pomodoro timer (wall-clock based, catches up after sleep)
│   │   ├── hub.rs           tasks, history, rewards, stats, import/export/backups
│   │   ├── images.rs        shrinks a chosen background to the screen (GIF frames too)
│   │   ├── persistence.rs   focushub_data.json, saved safely; daily backups
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
├── integration_test/        18 end-to-end tests on a phone/emulator, plus perf_test.dart
└── assets/                  background GIF and app icon (from the desktop app)
```

## Run it

```
flutter run                      # on a connected phone or the emulator
flutter test integration_test    # the end-to-end tests (needs a device)
cd rust && cargo test            # the Rust tests
```

After changing anything in `rust/src/api/`, regenerate the bridge:

```
flutter_rust_bridge_codegen generate
```

`flutter run` makes a **debug** build, which is much slower than what users get (Dart
runs without compiling ahead). Judge speed with `flutter run --profile` or `--release`.

## Build for a phone

```
flutter build apk --release --split-per-abi
```

Install `build/app/outputs/flutter-apk/app-arm64-v8a-release.apk` on the phone.
Release builds are signed with the debug key (fine for your own phone; use your own
key for Google Play).

## Speed and battery

The app does nothing in the background: no ticking, no drawing. Android counts down in
the notification and fires the scheduled alerts. In the foreground:

- **Ticks come right after a number on screen changes** (Rust says when), so about once
  a second instead of on a fixed fast beat, and only the clock, timer and stats redraw.
- **Hidden tabs sleep.** Tabs stay alive (so scroll positions and typing survive), but a
  hidden tab doesn't redraw, and the background GIF stops when the Focus tab is hidden.
- **The GIF also stops** in Battery Saver, with Android's "Remove animations", or with
  Settings → *Animated background* off. It's the only continuous animation.
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
- **Notification buttons** run in a second, short-lived copy of the app. Both copies
  share `focushub_data.json`; before every change (and before saving when the app is
  hidden) Rust checks whether the other copy changed the file and loads it if so.
- **Home screen icon**: Android doesn't let apps add their own icon silently. On first
  launch the app offers it (Android then asks you to confirm); Settings has it too.
  Launchers such as Pixel's also add new apps by themselves when their "Add app icons to
  Home screen" option is on.
- **Uninstalling removes all data**: everything is in the app's private folder, and
  cloud backup is off (`allowBackup="false"` and `data_extraction_rules.xml`), so a
  reinstall starts fresh. Export a backup first if you want to keep it.
- **Windows**: `flutter build windows` currently fails because the notification
  plugin needs Visual Studio's "C++ ATL" component. Install it with the Visual
  Studio Installer (Individual components → "C++ ATL for latest build tools").
- `android/gradle.properties` turns off Kotlin incremental compilation: the project
  (D:) and Flutter's package cache (C:) are on different drives, which breaks it.
