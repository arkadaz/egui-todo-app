# Focus Hub (Flutter + Rust)

A phone version of the desktop Focus Hub (`C:\Code\Rust\alarm\egui-todo-app`):
a Pomodoro timer over an animated background, to-dos per day with a calendar and
history, study stats, and rewards.

**Flutter draws the UI. Rust does everything else**: the data, saving, the timer,
stats, dates and the calendar. They talk through
[flutter_rust_bridge](https://cjycode.com/flutter_rust_bridge/).

## Layout

```
focus_hub/
├── rust/src/
│   ├── engine/            ← all the logic, plain Rust (no Flutter), with 37 tests
│   │   ├── domain.rs        saved data (same JSON as the desktop app)
│   │   ├── timer.rs         Pomodoro timer (wall-clock based, catches up after sleep)
│   │   ├── hub.rs           tasks, rewards, stats, backgrounds, import/export
│   │   ├── persistence.rs   focushub_data.json, saved safely
│   │   └── dates.rs         date math and formatting
│   └── api/focus_hub.rs   ← the bridge: what Dart can call
├── lib/
│   ├── main.dart            startup
│   ├── app.dart             theme, tabs, app lifecycle
│   ├── controller.dart      connects the screens to Rust
│   ├── services/alerts.dart notifications (scheduled with Android's alarms)
│   ├── pages/               Focus, Tasks, Stats, Rewards
│   └── widgets/             calendar, settings sheet
├── integration_test/        10 end-to-end tests that run on a phone/emulator
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

## Build for a phone

```
flutter build apk --release --split-per-abi
```

Install `build/app/outputs/flutter-apk/app-arm64-v8a-release.apk` on the phone.
Release builds are signed with the debug key (fine for your own phone; use your own
key for Google Play).

## Notes

- **Your desktop data**: Settings → *Import from the desktop app…* reads a
  `focushub_data.json` from the egui app, and *Export a backup…* writes one the
  desktop app can open.
- **Alerts** are scheduled with Android's exact alarms when the timer starts, so
  they arrive on time even if the phone sleeps or the app is closed. The app uses
  `USE_EXACT_ALARM` (granted automatically); the Play Store only allows that
  permission for alarm, timer and calendar apps.
- The timer uses the real clock, so it keeps counting while the phone sleeps, and
  a running session survives the app being closed.
- **Windows**: `flutter build windows` currently fails because the notification
  plugin needs Visual Studio's "C++ ATL" component. Install it with the Visual
  Studio Installer (Individual components → "C++ ATL for latest build tools").
- `android/gradle.properties` turns off Kotlin incremental compilation: the project
  (D:) and Flutter's package cache (C:) are on different drives, which breaks it.
