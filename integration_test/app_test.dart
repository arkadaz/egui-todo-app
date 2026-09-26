import 'dart:io';
import 'dart:ui' as ui;

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_rust_bridge/flutter_rust_bridge.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:focus_hub/src/rust/api/focus_hub.dart';
import 'package:focus_hub/src/rust/frb_generated.dart';
import 'package:integration_test/integration_test.dart';

import 'helpers.dart';

/// A [width] x [height] PNG, drawn by Flutter.
Future<Uint8List> pngOf(int width, int height) async {
  final recorder = ui.PictureRecorder();
  Canvas(recorder).drawRect(
    Rect.fromLTWH(0, 0, width.toDouble(), height.toDouble()),
    Paint()..color = Colors.orange,
  );
  final image = await recorder.endRecording().toImage(width, height);
  final data = await image.toByteData(format: ui.ImageByteFormat.png);
  return data!.buffer.asUint8List();
}

/// Width and height of an image file.
Future<(int, int)> sizeOf(File file) async {
  final codec = await ui.instantiateImageCodec(file.readAsBytesSync());
  final frame = (await codec.getNextFrame()).image;
  return (frame.width, frame.height);
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

    await tester.tap(find.byType(Checkbox).first);
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

  testWidgets('settings: a background is resized by Rust, shown, and removed', (tester) async {
    final controller = await startApp(tester);
    expect(controller.backgroundPath, isNull);
    final dir = Directory.systemTemp.createTempSync('focus_hub_images');
    final notes = File('${dir.path}/notes.txt')..writeAsStringSync('not an image');
    final photo = File('${dir.path}/photo.png')..writeAsBytesSync((await tester.runAsync(() => pngOf(3000, 2000)))!);
    final duck = File('${dir.path}/duck.GIF')
      ..writeAsBytesSync((await rootBundle.load('assets/background.gif')).buffer.asUint8List());
    Future<String?> choose(File file, int screenSide) =>
        tester.runAsync(() => controller.setBackground(file.path, screenSide));

    expect(await choose(notes, 1080), 'Please choose a GIF, PNG, JPG or WebP image.');
    expect(controller.backgroundPath, isNull);

    // A photo bigger than the screen is shrunk to it.
    expect(await choose(photo, 1080), 'Background changed (resized from 3000 × 2000 to 1080 × 720 for this screen).');
    final photoCopy = controller.backgroundPath!;
    expect(await tester.runAsync(() => sizeOf(File(photoCopy))), (1080, 720));
    await tester.pump();
    expect(find.byWidgetPredicate((w) => w is Image && w.image is FileImage), findsOneWidget);

    // The duck GIF (383 × 480) already fits, so it's kept as it is.
    expect(await choose(duck, 2400), 'Background changed.');
    final duckCopy = controller.backgroundPath!;
    expect(duckCopy, endsWith('.gif'));
    expect(File(duckCopy).readAsBytesSync(), duck.readAsBytesSync());
    expect(File(photoCopy).existsSync(), isFalse, reason: 'the old copy is deleted');

    await tester.tap(find.byTooltip('Settings'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('Use the default background'));
    await tester.pumpAndSettle();
    expect(controller.backgroundPath, isNull);
    expect(File(duckCopy).existsSync(), isFalse, reason: 'our copy is deleted');
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
    await tester.scrollUntilVisible(find.text('Friday, July 25'), 200, scrollable: find.byType(Scrollable).first);
    expect(find.text('Saturday, July 25'), findsNothing, reason: 'July 25, 2025 was a Friday');
    await tester.tap(find.text('All 1 done')); // finished days are folded to one line
    await tester.pumpAndSettle();
    expect(find.text('Study Rust'), findsOneWidget);
  });

  testWidgets('data survives closing and reopening', (tester) async {
    final dir = Directory.systemTemp.createTempSync('focus_hub_test');
    final first = FocusHub.open(dataDir: dir.path);
    first.addTodo(date: todayDate(), text: 'Keep me');
    first.addReward(name: 'Prize');
    first.setTimerSettings(
      workSecs: 25 * 60,
      breakSecs: 5 * 60,
      longBreakSecs: 20 * 60,
      loops: 4,
      longBreakEvery: 2,
      autoStart: false,
    );

    final reopened = FocusHub.open(dataDir: dir.path);
    expect(reopened.todosFor(date: todayDate()).single.text, 'Keep me');
    expect(reopened.rewards().single.name, 'Prize');
    final timer = reopened.timerView();
    expect(
      (timer.workSecs, timer.breakSecs, timer.longBreakSecs, timer.loops, timer.longBreakEvery, timer.autoStart),
      (1500, 300, 1200, 4, 2, false),
    );
    expect(timer.remaining, '25:00');
    expect(File('${dir.path}/focushub_data.json').existsSync(), isTrue);
  });
}
