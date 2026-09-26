import 'dart:io' show Platform;

import 'package:flutter/foundation.dart';
import 'package:flutter_local_notifications/flutter_local_notifications.dart';
import 'package:timezone/data/latest_all.dart' as tz_data;
import 'package:timezone/timezone.dart' as tz;

import '../src/rust/api/focus_hub.dart';

/// Whether notifications are allowed.
typedef AlertStatus = ({bool notifications, bool onTime});

/// Session-end notifications, played with the desktop app's 440 Hz beep.
///
/// Notifications are scheduled with Android's alarm system when the timer starts, so they
/// arrive on time even if the phone is asleep or the app has been closed.
/// Only Android is set up; on other platforms these methods do nothing.
class Alerts {
  final _plugin = FlutterLocalNotificationsPlugin();

  static const _channel = AndroidNotificationChannel(
    'focus_timer',
    'Timer alerts',
    description: 'Tells you when a study session or break ends.',
    importance: Importance.max,
    playSound: true,
    sound: RawResourceAndroidNotificationSound('beep'),
    enableVibration: true,
  );

  static final _details = NotificationDetails(
    android: AndroidNotificationDetails(
      _channel.id,
      _channel.name,
      channelDescription: _channel.description,
      importance: Importance.max,
      priority: Priority.high,
      playSound: true,
      sound: const RawResourceAndroidNotificationSound('beep'),
      category: AndroidNotificationCategory.alarm,
      visibility: NotificationVisibility.public,
    ),
  );

  /// Scheduled alerts use ids from here up.
  static const _firstScheduledId = 100;
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
          )
          .timeout(const Duration(seconds: 3));
      await _android?.createNotificationChannel(_channel).timeout(const Duration(seconds: 3));
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
      notificationDetails: _details,
    );
  }

  /// Replaces all scheduled alerts with [upcoming] (empty when the timer is stopped).
  Future<void> schedule(List<ScheduledAlert> upcoming) async {
    if (!supported) return;
    try {
      for (final pending in await _plugin.pendingNotificationRequests()) {
        await _plugin.cancel(id: pending.id);
      }
      if (upcoming.isEmpty) return;
      final onTime = await _android?.canScheduleExactNotifications() ?? false;
      final mode =
          onTime ? AndroidScheduleMode.exactAllowWhileIdle : AndroidScheduleMode.inexactAllowWhileIdle;
      final now = tz.TZDateTime.now(tz.UTC);
      for (var i = 0; i < upcoming.length; i++) {
        final alert = upcoming[i];
        // Alerts due within a second are shown by the app itself (see HomeShell).
        if (alert.delayMs < 1000) continue;
        await _plugin.zonedSchedule(
          id: _firstScheduledId + i,
          title: alert.title,
          body: alert.body,
          scheduledDate: now.add(Duration(milliseconds: alert.delayMs)),
          notificationDetails: _details,
          androidScheduleMode: mode,
        );
      }
    } catch (e) {
      debugPrint('Could not schedule alerts: $e');
    }
  }
}
