import 'dart:io';

import 'package:flutter/cupertino.dart';
import 'package:flutter/material.dart';

import '../app.dart' show focusHubTheme;
import '../controller.dart';
import '../services/battery_saver.dart';
import '../src/rust/api/focus_hub.dart';
import '../widgets/live_builder.dart';
import '../widgets/settings_sheet.dart';

/// The desktop app's main window: clock and Pomodoro timer over an animated background.
class FocusPage extends StatelessWidget {
  const FocusPage({super.key, required this.controller});

  final FocusController controller;

  @override
  Widget build(BuildContext context) {
    // The panel is always dark over the background, whatever the phone's theme.
    final dark = focusHubTheme(Brightness.dark);
    return Theme(
      data: dark,
      child: Scaffold(
        backgroundColor: dark.colorScheme.surface,
        extendBodyBehindAppBar: true,
        appBar: AppBar(
          title: const Text('Focus Hub'),
          backgroundColor: Colors.black.withValues(alpha: 0.35),
          actions: [
            IconButton(
              tooltip: 'Settings',
              icon: const Icon(Icons.settings_outlined),
              onPressed: () => showSettingsSheet(context, controller),
            ),
          ],
        ),
        body: Stack(
          fit: StackFit.expand,
          children: [
            // A layer of its own: each GIF frame repaints only the picture. The GIF is the
            // app's only animation, so it stops when it's turned off in Settings, in Battery
            // Saver, or with Android's "Remove animations" (Image checks that itself).
            RepaintBoundary(
              child: ListenableBuilder(
                listenable: Listenable.merge([controller, batterySaver]),
                builder: (context, _) => TickerMode(
                  enabled: controller.animateBackground && !batterySaver.value,
                  child: BackgroundImage(path: controller.backgroundPath),
                ),
              ),
            ),
            SafeArea(
              child: Center(
                child: SingleChildScrollView(
                  padding: const EdgeInsets.all(16),
                  child: ConstrainedBox(
                    constraints: const BoxConstraints(maxWidth: 480),
                    child: TimerPanel(controller: controller),
                  ),
                ),
              ),
            ),
          ],
        ),
      ),
    );
  }
}

/// The chosen background image, or the built-in duck GIF, shown whole (like the desktop
/// app, whose window was sized to fit the GIF) on the dark background. A chosen image was
/// already shrunk to fit the screen by Rust (see `FocusController.setBackground`).
class BackgroundImage extends StatelessWidget {
  const BackgroundImage({super.key, required this.path});

  final String? path;

  @override
  Widget build(BuildContext context) {
    const fallback = Image(
      image: AssetImage('assets/background.gif'),
      fit: BoxFit.contain,
      gaplessPlayback: true,
    );
    final path = this.path;
    if (path == null) return fallback;
    return Image.file(
      File(path),
      fit: BoxFit.contain,
      gaplessPlayback: true,
      errorBuilder: (context, error, stack) => fallback,
    );
  }
}

/// The dark, see-through panel with the clock and timer.
class TimerPanel extends StatelessWidget {
  const TimerPanel({super.key, required this.controller});

  final FocusController controller;

  @override
  Widget build(BuildContext context) {
    return Container(
      padding: const EdgeInsets.fromLTRB(20, 20, 20, 8),
      decoration: BoxDecoration(
        color: const Color.fromARGB(180, 20, 20, 20),
        borderRadius: BorderRadius.circular(16),
      ),
      child: Column(
        mainAxisSize: MainAxisSize.min,
        children: [
          // Changes every second, so it's a layer of its own: the buttons and settings
          // below are neither rebuilt nor repainted with it.
          RepaintBoundary(child: _Countdown(controller: controller)),
          const SizedBox(height: 16),
          ListenableBuilder(
            listenable: controller,
            builder: (context, _) {
              final t = controller.timer;
              return Column(
                children: [
                  Wrap(
                    alignment: WrapAlignment.center,
                    spacing: 8,
                    runSpacing: 8,
                    children: [
                      FilledButton.icon(
                        onPressed: controller.toggleTimer,
                        icon: Icon(t.isRunning ? Icons.pause : Icons.play_arrow),
                        label: Text(t.isRunning ? 'Pause' : 'Start'),
                      ),
                      OutlinedButton.icon(
                        onPressed: t.isAtStart ? null : controller.skipSession,
                        icon: const Icon(Icons.skip_next),
                        label: Text(t.isWork ? 'Skip' : 'Skip break'),
                      ),
                      OutlinedButton.icon(
                        onPressed: t.isAtStart ? null : controller.resetTimer,
                        icon: const Icon(Icons.restart_alt),
                        label: const Text('Reset'),
                      ),
                    ],
                  ),
                  const SizedBox(height: 8),
                  TimerSettings(controller: controller, timer: t),
                ],
              );
            },
          ),
        ],
      ),
    );
  }
}

/// The clock, the session, the time left and the progress bar.
class _Countdown extends StatelessWidget {
  const _Countdown({required this.controller});

  final FocusController controller;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    const tabular = [FontFeature.tabularFigures()];
    return LiveBuilder(
      controller: controller,
      builder: (context) {
        final t = controller.timer;
        final modeColor = t.isWork ? theme.colorScheme.primary : theme.colorScheme.tertiary;
        return Column(
          children: [
            Text(controller.clock, style: const TextStyle(fontSize: 24, fontFeatures: tabular)),
            const SizedBox(height: 10),
            Text('Pomodoro Timer', style: theme.textTheme.headlineSmall),
            const SizedBox(height: 4),
            Text(
              '${t.modeLabel} ${t.loopLabel}',
              style: theme.textTheme.titleMedium?.copyWith(color: modeColor),
            ),
            Text(
              t.remaining,
              style: const TextStyle(fontSize: 64, fontWeight: FontWeight.w300, fontFeatures: tabular),
            ),
            Row(
              children: [
                Expanded(
                  child: LinearProgressIndicator(
                    value: t.progress,
                    minHeight: 8,
                    borderRadius: BorderRadius.circular(4),
                    color: modeColor,
                  ),
                ),
                const SizedBox(width: 12),
                SizedBox(
                  width: 44,
                  child: Text(
                    '${(t.progress * 100).floor()}%',
                    textAlign: TextAlign.end,
                    style: const TextStyle(fontFeatures: tabular),
                  ),
                ),
              ],
            ),
          ],
        );
      },
    );
  }
}

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
          child: Text(
            'Changing a setting resets the timer.',
            style: Theme.of(context).textTheme.bodySmall,
          ),
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
                mode: CupertinoTimerPickerMode.hms,
                initialTimerDuration: chosen,
                onTimerDurationChanged: (value) => chosen = value,
              ),
            ),
            const SizedBox(height: 8),
            SizedBox(
              width: double.infinity,
              child: FilledButton(
                onPressed: () => Navigator.pop(context, chosen.inSeconds),
                child: const Text('Save'),
              ),
            ),
          ],
        ),
      ),
    ),
  );
}
