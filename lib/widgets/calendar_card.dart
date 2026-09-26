import 'package:flutter/material.dart';

import '../controller.dart';
import '../src/rust/api/focus_hub.dart';

/// A month calendar (Rust works out the layout). Tap a day to edit its tasks.
/// A dot marks days that have tasks.
class CalendarCard extends StatelessWidget {
  const CalendarCard({super.key, required this.controller});

  final FocusController controller;

  static const _weekdays = ['Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat', 'Sun'];

  @override
  Widget build(BuildContext context) {
    final month = controller.calendar;
    final labelStyle = Theme.of(context).textTheme.labelSmall?.copyWith(fontWeight: FontWeight.bold);
    return Card(
      child: Padding(
        padding: const EdgeInsets.all(8),
        child: Column(
          children: [
            Row(
              children: [
                IconButton(
                  tooltip: 'Previous month',
                  icon: const Icon(Icons.chevron_left),
                  onPressed: () => controller.showMonth(-1),
                ),
                Expanded(
                  child: Text(
                    month.title,
                    textAlign: TextAlign.center,
                    style: Theme.of(context).textTheme.titleMedium,
                  ),
                ),
                TextButton(onPressed: controller.goToToday, child: const Text('Today')),
                IconButton(
                  tooltip: 'Next month',
                  icon: const Icon(Icons.chevron_right),
                  onPressed: () => controller.showMonth(1),
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
                for (var i = 0; i < month.leadingBlanks; i++) const SizedBox.shrink(),
                for (final day in month.days)
                  _DayCell(day: day, onTap: () => controller.selectDate(day.date)),
              ],
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
    return Semantics(
      button: true,
      selected: day.isSelected,
      label: '${day.date}${day.isToday ? ', today' : ''}${day.hasTodos ? ', has tasks' : ''}',
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
                    width: 4,
                    height: 4,
                    decoration: BoxDecoration(
                      shape: BoxShape.circle,
                      color: day.isToday ? colors.onPrimaryContainer : colors.onSurface,
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
