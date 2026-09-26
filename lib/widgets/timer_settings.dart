import 'package:flutter/cupertino.dart';
import 'package:flutter/material.dart';

import '../controller.dart';
import '../src/rust/api/focus_hub.dart';

/// "Timer Settings", folded away until opened (like the desktop app).
class TimerSettings extends StatelessWidget {
  const TimerSettings({super.key, required this.controller, required this.timer});

  final FocusController controller;
  final TimerView timer;

  @override
  Widget build(BuildContext context) {
    final t = timer;
    final hasLongBreaks = t.longBreakEvery > 0;
    return ExpansionTile(
      title: const Text('Timer Settings'),
      tilePadding: EdgeInsets.zero,
      shape: const Border(),
      collapsedShape: const Border(),
      children: [
        Align(
          alignment: Alignment.centerLeft,
          child: Wrap(
            spacing: 8,
            runSpacing: 4,
            children: [
              for (final preset in controller.timerPresets)
                ChoiceChip(
                  label: Text(preset.name),
                  selected: t.preset == preset.index,
                  onSelected: (_) => controller.applyPreset(preset.index),
                ),
            ],
          ),
        ),
        const SizedBox(height: 4),
        _DurationTile(
          icon: Icons.menu_book_outlined,
          title: 'Study time',
          secs: t.workSecs,
          limit: 'Up to 120 minutes',
          onChanged: (secs) => controller.setTimerSettings(workSecs: secs),
        ),
        _DurationTile(
          icon: Icons.coffee_outlined,
          title: 'Short break',
          secs: t.breakSecs,
          limit: 'Up to 60 minutes',
          onChanged: (secs) => controller.setTimerSettings(breakSecs: secs),
        ),
        _StepperTile(
          icon: Icons.repeat,
          title: 'Number of loops',
          value: '${t.loops}',
          onLess: t.loops > 1 ? () => controller.setTimerSettings(loops: t.loops - 1) : null,
          onMore: t.loops < 20 ? () => controller.setTimerSettings(loops: t.loops + 1) : null,
        ),
        _StepperTile(
          icon: Icons.weekend_outlined,
          title: 'Long break',
          subtitle: hasLongBreaks ? 'After every ${t.longBreakEvery} loops' : 'Never',
          value: hasLongBreaks ? 'every ${t.longBreakEvery}' : 'off',
          onLess: hasLongBreaks
              ? () => controller.setTimerSettings(longBreakEvery: t.longBreakEvery == 2 ? 0 : t.longBreakEvery - 1)
              : null,
          onMore: t.longBreakEvery < 20
              ? () => controller.setTimerSettings(longBreakEvery: hasLongBreaks ? t.longBreakEvery + 1 : 2)
              : null,
        ),
        if (hasLongBreaks)
          _DurationTile(
            icon: Icons.self_improvement,
            title: 'Long break length',
            secs: t.longBreakSecs,
            limit: 'Up to 60 minutes',
            onChanged: (secs) => controller.setTimerSettings(longBreakSecs: secs),
          ),
        SwitchListTile(
          contentPadding: EdgeInsets.zero,
          secondary: const Icon(Icons.play_circle_outline),
          title: const Text('Start next session automatically'),
          value: t.autoStart,
          onChanged: (on) => controller.setTimerSettings(autoStart: on),
        ),
        Padding(
          padding: const EdgeInsets.only(bottom: 8),
          child: Text('Changing a setting resets the timer.', style: Theme.of(context).textTheme.bodySmall),
        ),
      ],
    );
  }
}

class _DurationTile extends StatelessWidget {
  const _DurationTile({
    required this.icon,
    required this.title,
    required this.secs,
    required this.limit,
    required this.onChanged,
  });

  final IconData icon;
  final String title;
  final int secs;
  final String limit;
  final ValueChanged<int> onChanged;

  @override
  Widget build(BuildContext context) {
    return ListTile(
      contentPadding: EdgeInsets.zero,
      leading: Icon(icon),
      title: Text(title),
      trailing: Text(_minutesSeconds(secs)),
      onTap: () async {
        final chosen = await pickDuration(context, title: title, initialSecs: secs, limit: limit);
        if (chosen != null) onChanged(chosen);
      },
    );
  }
}

class _StepperTile extends StatelessWidget {
  const _StepperTile({
    required this.icon,
    required this.title,
    required this.value,
    required this.onLess,
    required this.onMore,
    this.subtitle,
  });

  final IconData icon;
  final String title;
  final String? subtitle;
  final String value;
  final VoidCallback? onLess;
  final VoidCallback? onMore;

  @override
  Widget build(BuildContext context) {
    return ListTile(
      contentPadding: EdgeInsets.zero,
      leading: Icon(icon),
      title: Text(title),
      subtitle: subtitle == null ? null : Text(subtitle!),
      trailing: Row(
        mainAxisSize: MainAxisSize.min,
        children: [
          IconButton(tooltip: 'Less', icon: const Icon(Icons.remove), onPressed: onLess),
          Text(value),
          IconButton(tooltip: 'More', icon: const Icon(Icons.add), onPressed: onMore),
        ],
      ),
    );
  }
}

/// "60m 00s"
String _minutesSeconds(int secs) => '${secs ~/ 60}m ${(secs % 60).toString().padLeft(2, '0')}s';

/// A wheel for choosing hours, minutes and seconds. Returns seconds, or `null` if cancelled.
Future<int?> pickDuration(
  BuildContext context, {
  required String title,
  required int initialSecs,
  required String limit,
}) {
  var chosen = Duration(seconds: initialSecs);
  return showModalBottomSheet<int>(
    context: context,
    showDragHandle: true,
    builder: (context) => SafeArea(
      child: Padding(
        padding: const EdgeInsets.fromLTRB(24, 0, 24, 16),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            Text(title, style: Theme.of(context).textTheme.titleLarge),
            Text(limit, style: Theme.of(context).textTheme.bodySmall),
            SizedBox(
              height: 216,
              child: CupertinoTimerPicker(
                initialTimerDuration: chosen,
                onTimerDurationChanged: (value) => chosen = value,
              ),
            ),
            const SizedBox(height: 8),
            SizedBox(
              width: double.infinity,
              child: FilledButton(onPressed: () => Navigator.pop(context, chosen.inSeconds), child: const Text('Save')),
            ),
          ],
        ),
      ),
    ),
  );
}
