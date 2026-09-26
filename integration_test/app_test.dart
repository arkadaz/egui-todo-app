import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_rust_bridge/flutter_rust_bridge.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:focus_hub/app.dart';
import 'package:focus_hub/controller.dart';
import 'package:focus_hub/services/alerts.dart';
import 'package:focus_hub/src/rust/api/focus_hub.dart';
import 'package:focus_hub/src/rust/frb_generated.dart';
import 'package:integration_test/integration_test.dart';

/// Alerts that do nothing, so tests never open permission popups.
class SilentAlerts extends Alerts {
  @override
  bool get supported => false;
}

/// Opens the app on a fresh, empty data folder.
Future<FocusController> startApp(WidgetTester tester) async {
  final dir = Directory.systemTemp.createTempSync('focus_hub_test');
  final controller = FocusController(hub: FocusHub.open(dataDir: dir.path), alerts: SilentAlerts());
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

Future<void> openTab(WidgetTester tester, String label) async {
  await tester.tap(find.descendant(of: find.byType(NavigationBar), matching: find.text(label)));
  await tester.pumpAndSettle();
}

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();
  setUpAll(() async => await RustLib.init());

  testWidgets('the Focus tab shows the Pomodoro timer', (tester) async {
    await startApp(tester);
    expect(find.text('Pomodoro Timer'), findsOneWidget);
    expect(find.text('Study Time (1/1)'), findsOneWidget);
    expect(find.text('60:00'), findsOneWidget, reason: 'default study time is 60 minutes');
    expect(find.text('Start'), findsOneWidget);
  });

  testWidgets('start counts down, pause stops, reset goes back', (tester) async {
    await startApp(tester);
    await tester.tap(find.text('Start'));
    await tester.pump();
    expect(find.text('Pause'), findsOneWidget);
    await wait(tester, const Duration(milliseconds: 1600));
    expect(find.text('60:00'), findsNothing, reason: 'the timer should be counting down');

    await tester.tap(find.text('Pause'));
    await tester.pump();
    final paused = tester.widget<Text>(find.textContaining(RegExp(r'^\d\d:\d\d$')).last).data;
    await wait(tester, const Duration(milliseconds: 1500));
    expect(find.text(paused!), findsOneWidget, reason: 'a paused timer must not move');

    await tester.tap(find.text('Reset'));
    await tester.pump();
    expect(find.text('60:00'), findsOneWidget);
    expect(find.text('Start'), findsOneWidget);
  });

  testWidgets('a finished session shows an alert and updates the stats', (tester) async {
    final controller = await startApp(tester);
    await controller.setTimerSettings(workSecs: 2, breakSecs: 1, loops: 1);
    await tester.pump();
    expect(find.text('00:02'), findsOneWidget);

    await tester.tap(find.text('Start'));
    await tester.pump();
    await wait(tester, const Duration(milliseconds: 2400));
    await tester.pump(const Duration(milliseconds: 300)); // let the alert slide in
    expect(find.text('Work Complete!'), findsOneWidget);
    expect(find.text('Break Time (1/1)'), findsOneWidget);

    await wait(tester, const Duration(milliseconds: 1200));
    await tester.pump(const Duration(milliseconds: 300));
    expect(find.text('All Sessions Done!'), findsOneWidget);
    expect(find.text('Start'), findsOneWidget, reason: 'after the last loop the timer stops');

    await openTab(tester, 'Stats');
    expect(find.text('Time studied'), findsOneWidget);
    expect(find.text('00:00:02'), findsOneWidget, reason: '2 seconds of study');
    expect(find.text('1'), findsWidgets, reason: 'one session completed');
  });

  testWidgets('tasks: add, complete, remove, and calendar dots', (tester) async {
    final semantics = tester.ensureSemantics(); // lets the test read the calendar's labels
    final controller = await startApp(tester);
    await openTab(tester, 'Tasks');
    expect(find.text('No tasks for this day.'), findsOneWidget);

    // Empty text is refused with a message from Rust.
    await tester.tap(find.byTooltip('Add task'));
    await tester.pump();
    expect(find.text('Type a task first.'), findsOneWidget);

    await tester.enterText(find.byType(TextField), 'Write tests');
    await tester.tap(find.byTooltip('Add task'));
    await tester.pump();
    await tester.enterText(find.byType(TextField), 'Read docs');
    await tester.tap(find.byTooltip('Add task'));
    await tester.pumpAndSettle();
    expect(find.text('Write tests'), findsOneWidget);
    expect(find.text('Read docs'), findsOneWidget);
    expect(find.bySemanticsLabel(RegExp(r'today, has tasks')), findsOneWidget);
    await closeKeyboard(tester);

    await tester.tap(find.text('Write tests'));
    await tester.pump();
    expect(controller.todos.first.completed, isTrue);

    await tester.tap(find.byTooltip('Remove task').last);
    await tester.pumpAndSettle();
    expect(find.text('Read docs'), findsNothing);
    expect(find.text('Write tests'), findsOneWidget);
    semantics.dispose();
  });

  testWidgets('earlier days appear in the task history', (tester) async {
    final controller = await startApp(tester);
    final today = controller.selectedDate;
    // Pick the day before today in the calendar (Rust does the date math).
    final days = controller.calendar.days;
    final index = days.indexWhere((d) => d.date == today);
    if (index > 0) {
      controller.selectDate(days[index - 1].date);
    } else {
      controller.showMonth(-1);
      controller.selectDate(controller.calendar.days.last.date);
    }
    controller.addTodo('Yesterday task');
    controller.selectDate(today);
    await openTab(tester, 'Tasks');
    // Scroll the page (the first scrollable; the calendar grid inside it is another one).
    await tester.scrollUntilVisible(find.text('Yesterday task'), 200, scrollable: find.byType(Scrollable).first);
    expect(find.text('Yesterday task'), findsOneWidget);
    expect(find.text('No tasks from previous days.'), findsNothing);
  });

  testWidgets('rewards: add, complete (moves last), remove', (tester) async {
    await startApp(tester);
    await openTab(tester, 'Rewards');
    await tester.enterText(find.byType(TextField), 'Cake');
    await tester.tap(find.byTooltip('Add reward'));
    await tester.pump();
    await tester.enterText(find.byType(TextField), 'Movie night');
    await tester.tap(find.byTooltip('Add reward'));
    await tester.pumpAndSettle();
    await closeKeyboard(tester);

    await tester.tap(find.text('Cake'));
    await tester.pumpAndSettle();
    final cake = tester.getTopLeft(find.text('Cake')).dy;
    final movie = tester.getTopLeft(find.text('Movie night')).dy;
    expect(cake, greaterThan(movie), reason: 'finished rewards go to the bottom');

    await tester.tap(find.byTooltip('Remove reward').first);
    await tester.pumpAndSettle();
    expect(find.text('Movie night'), findsNothing);
  });

  testWidgets('settings: the time zone changes the clock', (tester) async {
    final controller = await startApp(tester);
    expect(controller.timeZone.followsDevice, isTrue);
    expect(controller.timeZone.label, startsWith('Device (GMT'));

    await tester.tap(find.byTooltip('Settings'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('Time zone'));
    await tester.pumpAndSettle();
    await tester.scrollUntilVisible(find.text('GMT+9'), 100, scrollable: find.byType(Scrollable).last);
    await tester.tap(find.text('GMT+9'));
    await tester.pumpAndSettle();
    expect(find.text('GMT+09:00'), findsOneWidget, reason: 'the sheet shows the new zone');

    // The clock shows UTC+9 (compare hours and minutes; allow a minute to tick over).
    final expected = DateTime.now().toUtc().add(const Duration(hours: 9));
    final clock = controller.clock;
    final hhmm = '${expected.hour.toString().padLeft(2, '0')}:${expected.minute.toString().padLeft(2, '0')}';
    final later = expected.add(const Duration(minutes: 1));
    final hhmmLater = '${later.hour.toString().padLeft(2, '0')}:${later.minute.toString().padLeft(2, '0')}';
    expect(clock.startsWith(hhmm) || clock.startsWith(hhmmLater), isTrue, reason: 'clock was $clock');

    controller.setTimeZone(null);
    expect(controller.timeZone.followsDevice, isTrue);
  });

  testWidgets('settings: a custom background is copied, shown, and removed', (tester) async {
    final controller = await startApp(tester);
    expect(controller.backgroundPath, isNull);
    final gif = await rootBundle.load('assets/background.gif');
    expect(controller.setBackground('duck.GIF', gif.buffer.asUint8List()), isNull);
    expect(controller.setBackground('notes.txt', Uint8List.fromList([1, 2, 3])),
        'Please choose a GIF, PNG, JPG or WebP image.');
    final path = controller.backgroundPath!;
    expect(File(path).existsSync(), isTrue);
    await tester.pump();
    expect(find.byWidgetPredicate((w) => w is Image && w.image is FileImage), findsOneWidget);

    await tester.tap(find.byTooltip('Settings'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('Use the default background'));
    await tester.pumpAndSettle();
    expect(controller.backgroundPath, isNull);
    expect(File(path).existsSync(), isFalse, reason: 'our copy is deleted');
  });

  testWidgets('settings: import desktop data and export it again', (tester) async {
    final controller = await startApp(tester);
    const desktopJson = '''{
      "todos_by_date": { "2025-07-25": [ { "text": "Study Rust", "completed": true } ] },
      "stats": {
        "daily_study_seconds": { "2025-07-25": 7200 },
        "daily_streaks": { "2025-07-25": 2 },
        "monthly_streaks": { "2025-7": 2 }
      },
      "rewards": [ { "name": "Ice cream", "completed": false } ],
      "gif_path": "C:\\\\Users\\\\someone\\\\duck.gif"
    }''';
    final summary = controller.importJson(desktopJson);
    expect((summary.days, summary.tasks, summary.rewards), (1, 1, 1));
    expect(controller.stats.totalTime, '0d 2h 0m 0s');
    expect(controller.rewards.single.name, 'Ice cream');
    expect(controller.backgroundPath, isNull, reason: "the desktop's GIF path doesn't exist here");

    final exported = controller.exportJson()!;
    expect(exported, contains('"2025-07-25"'));
    expect(exported, contains('Study Rust'));
    expect(() => controller.importJson('not json'), throwsA(isA<AnyhowException>()));

    // The imported task shows up in the history of later days.
    await openTab(tester, 'Tasks');
    await tester.scrollUntilVisible(find.text('Study Rust'), 200, scrollable: find.byType(Scrollable).first);
    expect(find.text('Saturday, July 25'), findsNothing, reason: 'July 25, 2025 was a Friday');
    expect(find.text('Friday, July 25'), findsOneWidget);
  });

  testWidgets('data survives closing and reopening', (tester) async {
    final dir = Directory.systemTemp.createTempSync('focus_hub_test');
    final first = FocusHub.open(dataDir: dir.path);
    first.addTodo(date: todayDate(), text: 'Keep me');
    first.addReward(name: 'Prize');
    first.setTimerSettings(workSecs: 25 * 60, breakSecs: 5 * 60, loops: 4);

    final reopened = FocusHub.open(dataDir: dir.path);
    expect(reopened.todosFor(date: todayDate()).single.text, 'Keep me');
    expect(reopened.rewards().single.name, 'Prize');
    final timer = reopened.timerView();
    expect((timer.workSecs, timer.breakSecs, timer.loops), (1500, 300, 4));
    expect(timer.remaining, '25:00');
    expect(File('${dir.path}/focushub_data.json').existsSync(), isTrue);
  });
}
