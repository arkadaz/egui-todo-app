import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:focus_hub/app.dart';
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

/// Opens the app on a fresh, empty data folder. The background GIF stays still unless
/// [animate] (a GIF frame still on its way when a test ends counts as a leak).
Future<FocusController> startApp(WidgetTester tester, {bool animate = false}) async {
  final dir = Directory.systemTemp.createTempSync('focus_hub_test');
  final controller = FocusController(
    hub: FocusHub.open(dataDir: dir.path),
    alerts: SilentAlerts(),
    homeScreen: NoHomeScreen(),
  );
  if (!animate) controller.setAnimateBackground(false);
  await tester.pumpWidget(FocusHubApp(controller: controller));
  await tester.pumpAndSettle();
  return controller;
}

/// Lets real time pass (the timer uses the real clock), then redraws.
Future<void> wait(WidgetTester tester, Duration duration) async {
  await tester.runAsync(() => Future<void>.delayed(duration));
  await tester.pump();
}

/// Closes the on-screen keyboard and waits until the layout has stopped moving.
Future<void> closeKeyboard(WidgetTester tester) async {
  FocusManager.instance.primaryFocus?.unfocus();
  await wait(tester, const Duration(milliseconds: 800));
  await tester.pumpAndSettle();
}

Finder tabButton(String label) =>
    find.descendant(of: find.byType(NavigationBar), matching: find.text(label));

Future<void> openTab(WidgetTester tester, String label) async {
  await tester.tap(tabButton(label));
  await tester.pumpAndSettle();
}
