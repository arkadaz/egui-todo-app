import 'dart:io' show Platform;
import 'dart:ui' show DartPluginRegistrant;

import 'package:flutter/foundation.dart';
import 'package:flutter_local_notifications/flutter_local_notifications.dart';
import 'package:path_provider/path_provider.dart';
import 'package:timezone/data/latest_all.dart' as tz_data;
import 'package:timezone/timezone.dart' as tz;

import '../src/rust/api/focus_hub.dart';
import '../src/rust/frb_generated.dart';

/// Whether notifications are allowed.
typedef AlertStatus = ({bool notifications, bool onTime});

/// Everything about notifications:
///
/// * **Session-end alerts** with the desktop app's 440 Hz beep.
/// * A **live countdown** in the notification bar while the timer runs, with Pause/Skip
///   buttons (Resume/Reset when paused). Android draws the countdown itself.
///
/// When the timer starts, every future alert, and the countdown for every next session,
/// is scheduled with Android's alarm system. So they appear on time even if the phone is
/// asleep or the app has been closed. Only Android is set up; on other platforms these
/// methods do nothing.
class Alerts {
  final _plugin = FlutterLocalNotificationsPlugin();

  static const _alertChannel = AndroidNotificationChannel(
    'focus_timer',
    'Timer alerts',
    description: 'Tells you when a study session or break ends.',
    importance: Importance.max,
    playSound: true,
    sound: RawResourceAndroidNotificationSound('beep'),
    enableVibration: true,
  );

  static const _statusChannel = AndroidNotificationChannel(
    'focus_status',
    'Timer countdown',
    description: 'Shows the time left while the timer runs.',
    importance: Importance.low, // silent, and doesn't pop down over other apps
    playSound: false,
    enableVibration: false,
  );

  static final _alertDetails = NotificationDetails(
    android: AndroidNotificationDetails(
      _alertChannel.id,
      _alertChannel.name,
      channelDescription: _alertChannel.description,
      importance: Importance.max,
      priority: Priority.high,
      playSound: true,
      sound: const RawResourceAndroidNotificationSound('beep'),
      category: AndroidNotificationCategory.alarm,
      visibility: NotificationVisibility.public,
    ),
  );

  /// The live countdown (or "paused") notification.
  static const statusId = 50;

  /// Scheduled countdowns for the sessions after the current one use ids from here up...
  static const _firstScheduledStatusId = 60;

  /// ...and scheduled alerts from here up.
  static const _firstScheduledAlertId = 100;
  static const _testId = 1;

  bool get supported => !kIsWeb && Platform.isAndroid;

  AndroidFlutterLocalNotificationsPlugin? get _android =>
      _plugin.resolvePlatformSpecificImplementation<AndroidFlutterLocalNotificationsPlugin>();

  /// Sets up notifications. Never blocks the app for long: if the system doesn't answer
  /// within a few seconds, the app starts anyway (without alerts until it does).
  Future<void> init() async {
    if (!supported) return;
    try {
      tz_data.initializeTimeZones();
      await _plugin
          .initialize(
            settings: const InitializationSettings(
              android: AndroidInitializationSettings('@drawable/ic_stat_timer'),
            ),
            onDidReceiveBackgroundNotificationResponse: onNotificationButton,
          )
          .timeout(const Duration(seconds: 3));
      await _android?.createNotificationChannel(_alertChannel).timeout(const Duration(seconds: 3));
      await _android?.createNotificationChannel(_statusChannel).timeout(const Duration(seconds: 3));
    } catch (e) {
      debugPrint('Notifications unavailable: $e');
    }
  }

  /// Asks "Allow Focus Hub to send you notifications?" (only if not decided yet).
  Future<bool> askPermission() async {
    if (!supported) return false;
    return await _android?.requestNotificationsPermission() ?? false;
  }

  Future<AlertStatus> status() async {
    if (!supported) return (notifications: false, onTime: false);
    final notifications = await _android?.areNotificationsEnabled() ?? false;
    final onTime = await _android?.canScheduleExactNotifications() ?? false;
    return (notifications: notifications, onTime: onTime);
  }

  /// Opens the system screen for "Alarms & reminders" (on-time alerts).
  Future<void> askOnTimePermission() async {
    if (!supported) return;
    await _android?.requestExactAlarmsPermission();
  }

  /// Shows an alert right now, to hear the sound.
  Future<void> showTest() async {
    if (!supported) return;
    await _plugin.show(
      id: _testId,
      title: 'Focus Hub',
      body: 'This is how a session alert sounds.',
      notificationDetails: _alertDetails,
    );
  }

  /// Brings the notifications in line with the timer: the countdown shown now, and every
  /// alert and countdown still to come. Call after anything changes the timer.
  Future<void> sync(FocusHub hub) async {
    if (!supported) return;
    try {
      // Everything scheduled before is out of date.
      for (final pending in await _plugin.pendingNotificationRequests()) {
        await _plugin.cancel(id: pending.id);
      }
      await _showStatus(hub.statusNow());

      final events = hub.upcomingEvents();
      if (events.isEmpty) return;
      final onTime = await _android?.canScheduleExactNotifications() ?? false;
      final mode =
          onTime ? AndroidScheduleMode.exactAllowWhileIdle : AndroidScheduleMode.inexactAllowWhileIdle;
      final now = tz.TZDateTime.now(tz.UTC);
      for (var i = 0; i < events.length; i++) {
        final event = events[i];
        // Alerts due within a second are shown by the app itself (see HomeShell).
        if (event.delayMs < 1000) continue;
        final at = now.add(Duration(milliseconds: event.delayMs));
        await _plugin.zonedSchedule(
          id: _firstScheduledAlertId + i,
          title: event.alertTitle,
          body: event.alertBody,
          scheduledDate: at,
          notificationDetails: _alertDetails,
          androidScheduleMode: mode,
        );
        final next = event.nextStatus;
        if (next != null) {
          await _plugin.zonedSchedule(
            id: _firstScheduledStatusId + i,
            title: next.title,
            body: next.body,
            scheduledDate: at,
            notificationDetails: _statusDetails(next, postedAtMs: at.millisecondsSinceEpoch),
            androidScheduleMode: mode,
          );
        }
      }
    } catch (e) {
      debugPrint('Could not update notifications: $e');
    }
  }

  Future<void> _showStatus(TimerStatus? status) async {
    // Remove the countdowns of earlier sessions (they may have been posted by alarms).
    await _plugin.cancel(id: statusId);
    for (var id = _firstScheduledStatusId; id < _firstScheduledAlertId; id++) {
      await _plugin.cancel(id: id);
    }
    if (status == null) return;
    await _plugin.show(
      id: statusId,
      title: status.title,
      body: status.body,
      notificationDetails: _statusDetails(status, postedAtMs: DateTime.now().millisecondsSinceEpoch),
    );
  }

  /// A countdown notification. While running it can't be swiped away, Android draws the
  /// time left, and it disappears by itself when the session ends (the next session's
  /// countdown takes over).
  static NotificationDetails _statusDetails(TimerStatus status, {required int postedAtMs}) {
    // How long the notification stays after it appears: until its session ends.
    final leftMs = status.endsAtMs - postedAtMs;
    return NotificationDetails(
      android: AndroidNotificationDetails(
        _statusChannel.id,
        _statusChannel.name,
        channelDescription: _statusChannel.description,
        importance: Importance.low,
        priority: Priority.low,
        playSound: false,
        enableVibration: false,
        onlyAlertOnce: true,
        ongoing: status.running,
        autoCancel: false,
        category: AndroidNotificationCategory.stopwatch,
        visibility: NotificationVisibility.public,
        showWhen: status.running,
        when: status.running ? status.endsAtMs : null,
        usesChronometer: status.running,
        chronometerCountDown: status.running,
        timeoutAfter: status.running && leftMs > 0 ? leftMs : null,
        actions: status.running
            ? const [
                AndroidNotificationAction('pause', 'Pause', cancelNotification: false),
                AndroidNotificationAction('skip', 'Skip', cancelNotification: false),
              ]
            : const [
                AndroidNotificationAction('resume', 'Resume', cancelNotification: false),
                AndroidNotificationAction('reset', 'Reset', cancelNotification: false),
              ],
      ),
    );
  }
}

bool _rustReady = false;

/// Runs when a notification button (Pause, Skip, Resume, Reset) is tapped. Android runs
/// this in a separate, short-lived copy of the app, even when the app is closed. It opens
/// the saved data, changes the timer, and updates the notifications. The main app picks
/// up the change the next time it looks (see Hub::reload_if_changed in Rust).
@pragma('vm:entry-point')
Future<void> onNotificationButton(NotificationResponse response) async {
  DartPluginRegistrant.ensureInitialized(); // plugins work in this background copy too
  if (!_rustReady) {
    await RustLib.init();
    _rustReady = true;
  }
  final dir = await getApplicationSupportDirectory();
  final hub = FocusHub.open(dataDir: dir.path);
  final running = hub.timerView().isRunning;
  switch (response.actionId) {
    case 'pause' when running:
    case 'resume' when !running:
      hub.toggleTimer();
    case 'skip':
      hub.skipSession();
    case 'reset':
      hub.resetTimer();
  }
  final alerts = Alerts();
  await alerts.init();
  await alerts.sync(hub);
}
