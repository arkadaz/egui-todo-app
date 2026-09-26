// The controller with the real Rust engine and SQLite database, run on this computer.
// Needs the Rust library built for it first: `cargo build --release` in rust/.

import 'dart:io';

import 'package:flutter_test/flutter_test.dart';
import 'package:focus_hub/src/rust/api/focus_hub.dart';
import 'package:focus_hub/src/rust/frb_generated.dart';

import 'fakes.dart';

void main() {
  setUpAll(() async => RustLib.init());

  test('tasks are saved in the database and survive reopening', () {
    final dir = Directory.systemTemp.createTempSync('focus_hub_unit');
    final controller = testController(dir);
    expect(controller.addTodo('Write tests'), isNull);
    expect(controller.addTodo('   '), 'Type a task first.');
    controller.addTodo('Ship it');
    controller.setTodoCompleted(controller.selectedDate, 0, true);

    expect(File('${dir.path}/focushub.db').existsSync(), isTrue);
    final reopened = FocusHub.open(dataDir: dir.path);
    final todos = reopened.todosFor(date: todayDate());
    expect(todos.map((t) => (t.text, t.completed)), [('Write tests', true), ('Ship it', false)]);
  });

  test('a delete can be undone, back in its place', () {
    final controller = testController();
    for (final text in ['A', 'B', 'C']) {
      controller.addTodo(text);
    }
    final undo = controller.deleteTodo(1)!;
    expect(controller.todos.map((t) => t.text), ['A', 'C']);
    undo();
    expect(controller.todos.map((t) => t.text), ['A', 'B', 'C']);
  });

  test('unfinished tasks from earlier days move to today', () {
    final controller = testController();
    importData(
      controller,
      todos: {
        daysAgo(1): [task('Yesterday'), task('Done yesterday', done: true)],
        daysAgo(9): [task('Last week')],
      },
    );
    expect(controller.unfinishedBeforeToday, 2);
    expect(controller.moveUnfinishedToToday(), 2);
    expect(controller.todos.map((t) => t.text), ['Last week', 'Yesterday'], reason: 'oldest first');
    expect(controller.unfinishedBeforeToday, 0);
  });

  test('the history is grouped by month, filtered and searched', () {
    final controller = testController();
    importData(
      controller,
      todos: {
        daysAgo(3): [task('Buy milk', done: true), task('Write report')],
        daysAgo(40): [task('Old report', done: true)],
      },
    );
    final all = controller.history();
    expect(all.length, 2, reason: '37 days apart: two months');
    expect(all.first.days.single.summary, '1 of 2 done');
    expect(all.last.summary, '1 day · all done');

    final unfinished = controller.history(filter: HistoryFilter.unfinished);
    expect(unfinished.single.days.single.todos.single.text, 'Write report');

    final found = controller.history(search: 'REPORT');
    expect(found.expand((m) => m.days).expand((d) => d.todos).map((t) => t.text), ['Write report', 'Old report']);
  });

  test('a tick says when the next number on screen changes', () {
    final controller = testController();
    final first = controller.hub.tick();
    expect(first.redraw, isTrue, reason: 'nothing shown yet');
    expect(first.nextTickMs, inInclusiveRange(1, 1005));
    expect(first.ended, isEmpty);
  });

  test("the notification button's copy of the app is picked up", () {
    final dir = Directory.systemTemp.createTempSync('focus_hub_unit');
    final controller = testController(dir);
    expect(controller.hub.tick().reloaded, isFalse);

    // Android runs notification buttons in a second copy of the app, with its own connection.
    FocusHub.open(dataDir: dir.path).toggleTimer();
    final tick = controller.hub.tick();
    expect(tick.reloaded, isTrue);
    expect(controller.timer.isRunning, isTrue);
    expect(controller.hub.tick().reloaded, isFalse, reason: 'only once');
  });

  test("the desktop app's data imports and exports", () {
    final controller = testController();
    const desktop = '''{
      "todos_by_date": { "2025-07-25": [ { "text": "Study Rust", "completed": true } ] },
      "stats": { "daily_study_seconds": { "2025-07-25": 7200 }, "daily_streaks": { "2025-07-25": 2 },
                 "monthly_streaks": { "2025-7": 2 } },
      "rewards": [ { "name": "Ice cream", "completed": false } ]
    }''';
    final summary = controller.importJson(desktop);
    expect((summary.days, summary.tasks, summary.rewards), (1, 1, 1));
    expect(controller.stats.totalTime, '0d 2h 0m 0s');
    expect(controller.exportJson(), contains('"Study Rust"'));
  });
}
