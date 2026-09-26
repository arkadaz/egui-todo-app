// The Tasks page with the real Rust engine, run on this computer.
// Needs the Rust library built for it first: `cargo build --release` in rust/.

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:focus_hub/controller.dart';
import 'package:focus_hub/pages/tasks_page.dart';
import 'package:focus_hub/src/rust/frb_generated.dart';

import 'fakes.dart';

/// Scrolls [finder] into view, then taps it.
Future<void> tapInView(WidgetTester tester, Finder finder) async {
  await tester.ensureVisible(finder);
  await tester.pump();
  await tester.tap(finder);
  await tester.pump();
}

void main() {
  setUpAll(() async => RustLib.init());

  Future<FocusController> showTasks(WidgetTester tester) async {
    // A phone-sized screen, so the history is in view.
    tester.view.physicalSize = const Size(1080, 2400);
    tester.view.devicePixelRatio = 2.625;
    addTearDown(tester.view.reset);
    final controller = testController();
    importData(
      controller,
      todos: {
        daysAgo(3): [task('Buy milk', done: true), task('Write report')],
        daysAgo(40): [task('Old report', done: true)],
      },
    );
    await tester.pumpWidget(MaterialApp(home: TasksPage(controller: controller)));
    return controller;
  }

  testWidgets('the calendar shows one week and opens to the month', (tester) async {
    final controller = await showTasks(tester);
    expect(find.byTooltip('Next week'), findsOneWidget);
    await tester.tap(find.byTooltip('Show the whole month'));
    await tester.pump();
    expect(find.text(controller.calendar.title), findsOneWidget);
    expect(find.byTooltip('Next month'), findsOneWidget);
  });

  testWidgets('the newest month is open and finished days are folded', (tester) async {
    await showTasks(tester);
    await tester.scrollUntilVisible(find.text('1 day · all done'), 300, scrollable: find.byType(Scrollable).first);
    expect(find.text('Write report'), findsOneWidget);
    expect(find.text('Old report'), findsNothing, reason: 'older months start folded');

    await tapInView(tester, find.text('1 day · all done'));
    expect(find.text('All 1 done'), findsOneWidget);
    await tapInView(tester, find.text('All 1 done'));
    await tester.ensureVisible(find.text('Old report'));
    expect(find.text('Old report'), findsOneWidget);
  });
}
