import 'dart:convert';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:focus_hub/controller.dart';
import 'package:focus_hub/pages/focus_page.dart';
import 'package:focus_hub/src/rust/api/focus_hub.dart';
import 'package:focus_hub/src/rust/frb_generated.dart';
import 'package:integration_test/integration_test.dart';

import 'helpers.dart';

/// "2026-09-23" for [days] days before today.
String daysAgo(int days) {
  final now = DateTime.now();
  final d = DateTime(now.year, now.month, now.day - days);
  return '${d.year}-${d.month.toString().padLeft(2, '0')}-${d.day.toString().padLeft(2, '0')}';
}

/// Replaces the data with desktop-format JSON.
void importData(FocusController controller, {Map<String, Object> todos = const {}, Map<String, int> study = const {}}) {
  controller.importJson(jsonEncode({
    'todos_by_date': todos,
    'stats': {'daily_study_seconds': study},
    'rewards': <Object>[],
  }));
}

Map<String, Object> task(String text, {bool done = false}) => {'text': text, 'completed': done};

Future<void> tapVisible(WidgetTester tester, Finder finder) async {
  await tester.ensureVisible(finder);
  await tester.pumpAndSettle();
  await tester.tap(finder);
  await tester.pumpAndSettle();
}

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();
  setUpAll(() async => await RustLib.init());

  testWidgets('timer: a preset, then auto-start off and skipping the break', (tester) async {
    final controller = await startApp(tester);
    await tester.tap(find.text('Timer Settings'));
    await tester.pumpAndSettle();
    await tapVisible(tester, find.text('Classic Pomodoro · 25/5 ×4'));
    expect(find.text('25:00'), findsOneWidget);
    expect(find.text('Study Time (1/4)'), findsOneWidget);

    await controller.setTimerSettings(workSecs: 2, breakSecs: 60, loops: 2, longBreakEvery: 0, autoStart: false);
    await tester.pump();
    await tapVisible(tester, find.text('Start'));
    await wait(tester, const Duration(milliseconds: 2500));
    expect(find.text('Break Time (1/2)'), findsOneWidget);
    expect(find.text('01:00'), findsOneWidget, reason: 'with auto-start off, the break waits');
    expect(find.text('Start'), findsOneWidget);

    await tapVisible(tester, find.text('Skip break'));
    expect(find.text('Study Time (2/2)'), findsOneWidget);
    expect(controller.stats.todaySessions, 1, reason: 'a skipped break still completes the session');
  });

  testWidgets('timer: a long break after every 2 loops', (tester) async {
    final controller = await startApp(tester);
    await controller.setTimerSettings(
      workSecs: 1,
      breakSecs: 1,
      longBreakSecs: 30,
      loops: 2,
      longBreakEvery: 2,
      autoStart: true,
    );
    await tester.pump();
    await tapVisible(tester, find.text('Start'));
    // Work 1 s, short break 1 s, work 1 s, then the long break.
    await wait(tester, const Duration(milliseconds: 3600));
    expect(find.text('Long Break (2/2)'), findsOneWidget);
  });

  testWidgets('tasks: edit, undo a delete, reorder, and carry over', (tester) async {
    final controller = await startApp(tester);
    importData(controller, todos: {
      daysAgo(2): [task('Old one'), task('Old done', done: true)],
      daysAgo(5): [task('Older one')],
    });
    await openTab(tester, 'Tasks');
    controller.addTodo('A');
    controller.addTodo('B');
    await tester.pumpAndSettle();

    await tester.tap(find.text('A'));
    await tester.pumpAndSettle();
    expect(find.text('Edit task'), findsOneWidget);
    await tester.enterText(find.descendant(of: find.byType(AlertDialog), matching: find.byType(TextField)), 'A2');
    await tester.tap(find.text('Save'));
    await tester.pumpAndSettle();
    expect(controller.todos.map((t) => t.text), ['A2', 'B']);

    await tester.tap(find.byTooltip('Remove task').first);
    await tester.pumpAndSettle();
    expect(controller.todos.map((t) => t.text), ['B']);
    await tester.tap(find.text('Undo'));
    await tester.pumpAndSettle();
    expect(controller.todos.map((t) => t.text), ['A2', 'B'], reason: 'back in its place');

    controller.moveTodo(0, 1);
    await tester.pumpAndSettle();
    expect(tester.getTopLeft(find.text('B')).dy, lessThan(tester.getTopLeft(find.text('A2')).dy));

    await tapVisible(tester, find.text('Move to today'));
    expect(find.text('Moved 2 unfinished tasks to today.'), findsOneWidget);
    expect(controller.todos.map((t) => t.text), ['B', 'A2', 'Older one', 'Old one']);
  });

  testWidgets('tasks: the calendar folds to a week and opens to the month', (tester) async {
    final controller = await startApp(tester);
    await openTab(tester, 'Tasks');
    final today = controller.selectedDate;
    expect(find.byTooltip('Next week'), findsOneWidget, reason: 'one week at first');

    await tester.tap(find.byTooltip('Next week'));
    await tester.pumpAndSettle();
    expect(controller.selectedDate, addDays(date: today, days: 7));
    await tester.tap(find.byTooltip('Previous week'));
    await tester.pumpAndSettle();
    expect(controller.selectedDate, today);

    await tester.tap(find.byTooltip('Show the whole month'));
    await tester.pumpAndSettle();
    expect(find.byTooltip('Next month'), findsOneWidget);
    expect(find.text(controller.calendar.title), findsOneWidget);
    await tester.tap(find.byTooltip('Show one week'));
    await tester.pumpAndSettle();
    expect(find.byTooltip('Next week'), findsOneWidget);
  });

  testWidgets('tasks: history by month, filtered and searched', (tester) async {
    final controller = await startApp(tester);
    // 37 days apart, so always in different months.
    importData(controller, todos: {
      daysAgo(3): [task('Buy milk', done: true), task('Write report')],
      daysAgo(40): [task('Old report', done: true)],
    });
    await openTab(tester, 'Tasks');
    await tester.ensureVisible(find.text('Task History'));
    await tester.pumpAndSettle();

    // The newest month is open, older ones are folded.
    expect(find.text('Write report'), findsOneWidget);
    expect(find.text('1 of 2 done'), findsOneWidget);
    expect(find.text('1 day · 1 unfinished'), findsOneWidget);
    expect(find.text('Old report'), findsNothing);
    await tapVisible(tester, find.text('1 day · all done'));
    expect(find.text('All 1 done'), findsOneWidget);
    expect(find.text('Old report'), findsNothing, reason: 'finished days are folded to one line');
    await tapVisible(tester, find.text('All 1 done'));
    expect(find.text('Old report'), findsOneWidget);

    await tapVisible(tester, find.text('Unfinished'));
    expect(find.text('Write report'), findsOneWidget);
    expect(find.text('Buy milk'), findsNothing);
    expect(find.text('Old report'), findsNothing, reason: 'its month has nothing unfinished');

    await tapVisible(tester, find.text('All'));
    await tapVisible(tester, find.byTooltip('Search earlier tasks'));
    await tester.enterText(find.byType(TextField).last, 'REPORT');
    await tester.pumpAndSettle();
    expect(find.text('Write report'), findsOneWidget);
    expect(find.text('Old report'), findsOneWidget, reason: 'searching opens every month');
    expect(find.text('Buy milk'), findsNothing);
    await tester.enterText(find.byType(TextField).last, 'nothing like this');
    await tester.pumpAndSettle();
    expect(find.text('No earlier tasks match "nothing like this".'), findsOneWidget);

    await tapVisible(tester, find.byTooltip('Stop searching'));
    await tapVisible(tester, find.text('Open').first);
    expect(controller.selectedDate, daysAgo(3), reason: '"Open" shows that day in the editor');
  });

  testWidgets('rewards: rename, and undo a delete', (tester) async {
    final controller = await startApp(tester);
    await openTab(tester, 'Rewards');
    controller.addReward('Cake');
    await tester.pumpAndSettle();

    await tester.tap(find.byTooltip('Rename reward'));
    await tester.pumpAndSettle();
    await tester.enterText(find.descendant(of: find.byType(AlertDialog), matching: find.byType(TextField)), 'Cheesecake');
    await tester.tap(find.text('Save'));
    await tester.pumpAndSettle();
    expect(find.text('Cheesecake'), findsOneWidget);

    await tester.tap(find.byTooltip('Remove reward'));
    await tester.pumpAndSettle();
    expect(find.text('Cheesecake'), findsNothing);
    await tester.tap(find.text('Undo'));
    await tester.pumpAndSettle();
    expect(find.text('Cheesecake'), findsOneWidget);
  });

  testWidgets('stats: streak and study chart', (tester) async {
    final controller = await startApp(tester);
    importData(controller, study: {daysAgo(0): 1800, daysAgo(1): 3600, daysAgo(2): 120, daysAgo(20): 5400});
    await openTab(tester, 'Stats');
    expect(find.text('3-day streak'), findsOneWidget);
    expect(find.text('Top bar: 1h 0m'), findsOneWidget, reason: 'the 7-day chart');
    await tester.tap(find.text('30 days'));
    await tester.pumpAndSettle();
    expect(find.text('Top bar: 1h 30m'), findsOneWidget, reason: '20 days ago is in the 30-day chart');
  });

  testWidgets('battery: the background animation can be turned off', (tester) async {
    final controller = await startApp(tester, animate: true);
    bool animating() => tester
        .widget<TickerMode>(find.ancestor(of: find.byType(BackgroundImage), matching: find.byType(TickerMode)).first)
        .enabled;
    expect(animating(), isTrue);

    await tester.tap(find.byTooltip('Settings'));
    await tester.pumpAndSettle();
    await tapVisible(tester, find.text('Animated background'));
    expect(controller.animateBackground, isFalse);
    expect(animating(), isFalse);

    // Hidden tabs never animate.
    await tester.tapAt(const Offset(10, 10)); // close the sheet
    await tester.pumpAndSettle();
    controller.setAnimateBackground(true);
    await openTab(tester, 'Tasks');
    final focusTab = tester.widget<TickerMode>(
      find
          .ancestor(
            of: find.byType(FocusPage, skipOffstage: false),
            matching: find.byType(TickerMode, skipOffstage: false),
          )
          .first,
    );
    expect(focusTab.enabled, isFalse);
  });
}
