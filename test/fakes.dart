// Shared by the host tests (test/) and the on-device tests (integration_test/).

import 'dart:convert';
import 'dart:io';

import 'package:focus_hub/controller.dart';
import 'package:focus_hub/services/alerts.dart';
import 'package:focus_hub/services/home_screen.dart';
import 'package:focus_hub/src/rust/api/focus_hub.dart';

/// Alerts that do nothing, so tests never open permission popups.
class SilentAlerts extends Alerts {
  @override
  bool get supported => false;
}

/// No Home screen icon offer, so it doesn't cover what the tests tap.
class NoHomeScreen extends HomeScreen {
  @override
  Future<bool> canAdd() async => false;
}

/// A controller on a fresh, empty data folder.
FocusController testController([Directory? dir]) {
  dir ??= Directory.systemTemp.createTempSync('focus_hub_test');
  return FocusController(hub: FocusHub.open(dataDir: dir.path), alerts: SilentAlerts(), homeScreen: NoHomeScreen());
}

/// "2026-09-23" for [days] days before today.
String daysAgo(int days) {
  final now = DateTime.now();
  final d = DateTime(now.year, now.month, now.day - days);
  return '${d.year}-${d.month.toString().padLeft(2, '0')}-${d.day.toString().padLeft(2, '0')}';
}

Map<String, Object> task(String text, {bool done = false}) => {'text': text, 'completed': done};

/// Replaces the data with desktop-format JSON.
void importData(
  FocusController controller, {
  Map<String, Object> todos = const {},
  Map<String, int> study = const {},
}) {
  controller.importJson(
    jsonEncode({
      'todos_by_date': todos,
      'stats': {'daily_study_seconds': study},
      'rewards': <Object>[],
    }),
  );
}
