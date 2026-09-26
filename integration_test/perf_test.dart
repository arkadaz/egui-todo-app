// Frame timings for a heavy user: months of tasks and study time, timer running.
//
// Run in profile mode (debug-mode numbers mean nothing):
//   flutter drive --profile --driver=test_driver/integration_test.dart \
//     --target=integration_test/perf_test.dart
// The results land in build/integration_response_data.json.

import 'dart:convert';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:focus_hub/src/rust/frb_generated.dart';
import 'package:integration_test/integration_test.dart';

import 'helpers.dart';

/// 120 days of history with 6 tasks each, daily study time, and 20 rewards.
String heavyUserJson() {
  String iso(DateTime d) =>
      '${d.year}-${d.month.toString().padLeft(2, '0')}-${d.day.toString().padLeft(2, '0')}';
  final today = DateTime.now();
  final todos = <String, Object>{};
  final study = <String, int>{};
  final sessions = <String, int>{};
  for (var d = 1; d <= 120; d++) {
    final day = iso(DateTime(today.year, today.month, today.day - d));
    todos[day] = [
      for (var i = 0; i < 6; i++) {'text': 'Task ${i + 1} of day $d', 'completed': i.isEven},
    ];
    study[day] = 1800 + (d * 977) % 9000;
    sessions[day] = 1 + d % 4;
  }
  return jsonEncode({
    'todos_by_date': todos,
    'stats': {'daily_study_seconds': study, 'daily_streaks': sessions},
    'rewards': [
      for (var i = 0; i < 20; i++) {'name': 'Reward ${i + 1}', 'completed': i % 3 == 0},
    ],
  });
}

void main() {
  final binding = IntegrationTestWidgetsFlutterBinding.ensureInitialized();
  setUpAll(() async => await RustLib.init());

  testWidgets('a heavy user: idle, switch tabs, scroll', (tester) async {
    final controller = await startApp(tester, animate: true);
    controller.importJson(heavyUserJson());
    await tester.pumpAndSettle();
    // Draw every frame the app asks for, like a real run (not only the ones the test pumps).
    binding.framePolicy = LiveTestWidgetsFlutterBindingFramePolicy.fullyLive;

    Future<void> idle(int ms) => tester.runAsync(() => Future<void>.delayed(Duration(milliseconds: ms)));
    Future<void> watch(String key, Future<void> Function() action) =>
        binding.watchPerformance(action, reportKey: key);
    Future<void> fling(double dy) async {
      await tester.fling(find.byType(Scrollable).first, Offset(0, dy), 2500);
      await idle(1500);
    }

    await tester.tap(find.text('Start'));
    await idle(500);

    await watch('focus_idle', () => idle(3000));

    await tester.tap(tabButton('Tasks'));
    await idle(800);
    await watch('tasks_scroll', () async {
      await fling(-1500);
      await fling(-1500);
      await fling(3000);
    });
    await watch('tasks_idle', () => idle(3000));

    await tester.tap(tabButton('Stats'));
    await idle(800);
    await watch('stats_idle', () => idle(3000));

    await watch('switch_tabs', () async {
      for (var round = 0; round < 2; round++) {
        for (final tab in ['Focus', 'Tasks', 'Stats', 'Rewards']) {
          await tester.tap(tabButton(tab));
          await idle(500);
        }
      }
    });

    await tester.tap(tabButton('Focus'));
    await idle(500);
    await tester.tap(find.text('Pause'));
    await idle(300);
  });
}
