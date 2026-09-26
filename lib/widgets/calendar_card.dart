import 'package:flutter/material.dart';

import '../controller.dart';
import '../src/rust/api/focus_hub.dart';

/// The calendar (Rust works out the layout): the selected day's week, or the whole month.
/// Tap a day to edit its tasks. A dot marks days with tasks: colored if some are unfinished,
/// grey if they're all done.
class CalendarCard extends StatelessWidget {
  const CalendarCard({super.key, required this.controller});

  final FocusController controller;

  static const _weekdays = ['Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat', 'Sun'];

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final expanded = controller.calendarExpanded;
    final String title;
    final List<CalendarDay> days;
    var leadingBlanks = 0;
    if (expanded) {
      final month = controller.calendar;
      (title, days, leadingBlanks) = (month.title, month.days, month.leadingBlanks);
    } else {
      final week = controller.calendarWeek;
      (title, days) = (week.title, week.days);
    }
    final labelStyle = theme.textTheme.labelSmall?.copyWith(fontWeight: FontWeight.bold);
    return Card(
      child: Padding(
        padding: const EdgeInsets.fromLTRB(8, 8, 8, 0),
        child: Column(
          children: [
            Row(
              children: [
                IconButton(
                  tooltip: expanded ? 'Previous month' : 'Previous week',
                  icon: const Icon(Icons.chevron_left),
                  onPressed: () => expanded ? controller.showMonth(-1) : controller.shiftWeek(-1),
                ),
                Expanded(
                  child: Text(
                    title,
                    textAlign: TextAlign.center,
                    style: theme.textTheme.titleMedium,
                  ),
                ),
                TextButton(onPressed: controller.goToToday, child: const Text('Today')),
                IconButton(
                  tooltip: expanded ? 'Next month' : 'Next week',
                  icon: const Icon(Icons.chevron_right),
                  onPressed: () => expanded ? controller.showMonth(1) : controller.shiftWeek(1),
                ),
              ],
            ),
            GridView.count(
              crossAxisCount: 7,
              shrinkWrap: true,
              physics: const NeverScrollableScrollPhysics(),
              childAspectRatio: 1.15,
              children: [
                for (final name in _weekdays) Center(child: Text(name, style: labelStyle)),
                for (var i = 0; i < leadingBlanks; i++) const SizedBox.shrink(),
                for (final day in days) _DayCell(day: day, onTap: () => controller.selectDate(day.date)),
              ],
            ),
            IconButton(
              tooltip: expanded ? 'Show one week' : 'Show the whole month',
              visualDensity: VisualDensity.compact,
              icon: Icon(expanded ? Icons.expand_less : Icons.expand_more),
              onPressed: controller.toggleCalendar,
            ),
          ],
        ),
      ),
    );
  }
}

class _DayCell extends StatelessWidget {
  const _DayCell({required this.day, required this.onTap});

  final CalendarDay day;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    final colors = Theme.of(context).colorScheme;
    final status = day.hasUnfinished ? ', unfinished tasks' : (day.hasTodos ? ', has tasks, all done' : '');
    return Semantics(
      button: true,
      selected: day.isSelected,
      label: '${day.date}${day.isToday ? ', today' : ''}${day.hasTodos ? ', has tasks' : ''}$status',
      excludeSemantics: true,
      child: InkWell(
        onTap: onTap,
        customBorder: const CircleBorder(),
        child: Container(
          margin: const EdgeInsets.all(2),
          decoration: BoxDecoration(
            shape: BoxShape.circle,
            color: day.isToday ? colors.primaryContainer : null,
            border: day.isSelected ? Border.all(color: colors.primary, width: 2) : null,
          ),
          child: Stack(
            alignment: Alignment.center,
            children: [
              Text(
                '${day.day}',
                style: TextStyle(
                  fontWeight: day.isToday ? FontWeight.bold : null,
                  color: day.isToday ? colors.onPrimaryContainer : null,
                ),
              ),
              if (day.hasTodos)
                Positioned(
                  bottom: 4,
                  child: Container(
                    width: 5,
                    height: 5,
                    decoration: BoxDecoration(
                      shape: BoxShape.circle,
                      color: day.hasUnfinished
                          ? (day.isToday ? colors.onPrimaryContainer : colors.primary)
                          : colors.outline,
                    ),
                  ),
                ),
            ],
          ),
        ),
      ),
    );
  }
}
