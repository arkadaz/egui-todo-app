import 'package:flutter/material.dart';

import '../src/rust/api/focus_hub.dart';

// The Tasks page's history of earlier days, grouped by month.

/// "September 2026 · 12 days · 5 unfinished", tap to open or close the month.
class MonthHeader extends StatelessWidget {
  const MonthHeader({super.key, required this.month, required this.open, required this.onTap});

  final HistoryMonth month;
  final bool open;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Padding(
      padding: const EdgeInsets.only(top: 8),
      child: Material(
        color: theme.colorScheme.surfaceContainerHigh,
        borderRadius: BorderRadius.circular(12),
        child: ListTile(
          shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(12)),
          title: Text(month.title, style: theme.textTheme.titleMedium),
          subtitle: Text(month.summary),
          trailing: Icon(open ? Icons.expand_less : Icons.expand_more),
          onTap: onTap,
        ),
      ),
    );
  }
}

/// An earlier day in the history: its date, how much got done, and its tasks. A finished
/// day is folded to one line (tap it to see its tasks), so unfinished days stand out.
class HistoryDayCard extends StatelessWidget {
  const HistoryDayCard({
    super.key,
    required this.day,
    required this.showTasks,
    required this.onToggle,
    required this.onChanged,
    required this.onOpen,
  });

  final DayTodos day;
  final bool showTasks;
  final VoidCallback? onToggle;
  final void Function(int index, bool done) onChanged;
  final VoidCallback onOpen;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Padding(
          padding: const EdgeInsets.only(top: 8),
          child: InkWell(
            onTap: onToggle,
            borderRadius: BorderRadius.circular(8),
            child: Padding(
              padding: const EdgeInsets.only(left: 4),
              child: Row(
                children: [
                  Expanded(
                    child: Text(day.title, style: theme.textTheme.titleSmall?.copyWith(fontWeight: FontWeight.bold)),
                  ),
                  if (day.allDone) Icon(Icons.check_circle, size: 16, color: theme.colorScheme.primary),
                  const SizedBox(width: 4),
                  Text(day.summary, style: theme.textTheme.bodySmall),
                  if (onToggle != null) Icon(showTasks ? Icons.expand_less : Icons.expand_more, size: 20),
                  TextButton(onPressed: onOpen, child: const Text('Open')),
                ],
              ),
            ),
          ),
        ),
        if (showTasks)
          Card(
            margin: EdgeInsets.zero,
            child: Column(
              children: [
                for (final todo in day.todos)
                  CheckboxListTile(
                    dense: true,
                    value: todo.completed,
                    controlAffinity: ListTileControlAffinity.leading,
                    onChanged: (done) => onChanged(todo.index, done ?? false),
                    title: TaskText(text: todo.text, done: todo.completed),
                  ),
              ],
            ),
          ),
      ],
    );
  }
}

/// A task's text, crossed out when it's done.
class TaskText extends StatelessWidget {
  const TaskText({super.key, required this.text, required this.done});

  final String text;
  final bool done;

  @override
  Widget build(BuildContext context) {
    return Text(
      text,
      style: done
          ? TextStyle(decoration: TextDecoration.lineThrough, color: Theme.of(context).colorScheme.onSurfaceVariant)
          : null,
    );
  }
}
