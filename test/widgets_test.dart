// Widgets that don't need Rust.

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:focus_hub/pages/stats_page.dart';
import 'package:focus_hub/src/rust/api/focus_hub.dart';
import 'package:focus_hub/widgets/edit_helpers.dart';

void main() {
  Widget page(Widget child) => MaterialApp(home: Scaffold(body: child));

  testWidgets('the study chart labels the days and the top bar', (tester) async {
    const names = ['Sun', 'Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat'];
    final days = [
      for (var i = 0; i < 7; i++)
        ChartDay(date: '2026-09-${20 + i}', label: names[i], minutes: i * 10.0, isToday: i == 6),
    ];
    await tester.pumpWidget(page(StudyChart(days: days)));
    for (final name in names) {
      expect(find.text(name), findsOneWidget);
    }
    expect(find.text('Top bar: 1h 0m'), findsOneWidget);
  });

  testWidgets('an empty study chart says so', (tester) async {
    final days = [for (var i = 0; i < 30; i++) ChartDay(date: '2026-09-01', label: '$i', minutes: 0, isToday: false)];
    await tester.pumpWidget(page(StudyChart(days: days)));
    expect(find.text('No study yet in this period'), findsOneWidget);
    expect(find.text('5'), findsOneWidget, reason: 'the 30-day chart labels every 5th day');
    expect(find.text('6'), findsNothing);
  });

  testWidgets('the edit dialog shows errors and closes once saved', (tester) async {
    String? saved;
    await tester.pumpWidget(
      page(
        Builder(
          builder: (context) => TextButton(
            onPressed: () => showEditDialog(
              context,
              title: 'Edit task',
              initial: 'Old',
              save: (text) {
                if (text.trim().isEmpty) return 'Type a task first.';
                saved = text;
                return null;
              },
            ),
            child: const Text('open'),
          ),
        ),
      ),
    );
    await tester.tap(find.text('open'));
    await tester.pumpAndSettle();
    expect(find.text('Old'), findsOneWidget);

    await tester.enterText(find.byType(TextField), '  ');
    await tester.tap(find.text('Save'));
    await tester.pump();
    expect(find.text('Type a task first.'), findsOneWidget);
    expect(find.byType(AlertDialog), findsOneWidget, reason: 'stays open to fix it');

    await tester.enterText(find.byType(TextField), 'New');
    await tester.tap(find.text('Save'));
    await tester.pumpAndSettle();
    expect(saved, 'New');
    expect(find.byType(AlertDialog), findsNothing);
  });
}
