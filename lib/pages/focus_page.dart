import 'dart:io';

import 'package:flutter/cupertino.dart';
import 'package:flutter/material.dart';

import '../controller.dart';
import '../src/rust/api/focus_hub.dart';
import '../widgets/settings_sheet.dart';

/// The desktop app's main window: clock and Pomodoro timer over an animated background.
class FocusPage extends StatelessWidget {
  const FocusPage({super.key, required this.controller});

  final FocusController controller;

  @override
  Widget build(BuildContext context) {
    return Scaffold(
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
          ListenableBuilder(
            listenable: controller,
            builder: (context, _) => BackgroundImage(path: controller.backgroundPath),
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
    );
  }
}

/// The chosen background image, or the built-in duck GIF, shown whole (like the desktop
/// app, whose window was sized to fit the GIF) on the dark background.
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
    final theme = Theme.of(context);
    const tabular = [FontFeature.tabularFigures()];
    return ListenableBuilder(
      listenable: Listenable.merge([controller, controller.ticks]),
      builder: (context, _) {
        final t = controller.timer;
        final modeColor = t.isWork ? theme.colorScheme.primary : theme.colorScheme.tertiary;
        return Container(
          padding: const EdgeInsets.fromLTRB(20, 20, 20, 8),
          decoration: BoxDecoration(
            color: const Color.fromARGB(180, 20, 20, 20),
            borderRadius: BorderRadius.circular(16),
          ),
          child: Column(
            mainAxisSize: MainAxisSize.min,
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
              const SizedBox(height: 16),
              Row(
                mainAxisAlignment: MainAxisAlignment.center,
                children: [
                  FilledButton.icon(
                    onPressed: controller.toggleTimer,
                    icon: Icon(t.isRunning ? Icons.pause : Icons.play_arrow),
                    label: Text(t.isRunning ? 'Pause' : 'Start'),
                  ),
                  const SizedBox(width: 12),
                  OutlinedButton.icon(
                    onPressed: controller.resetTimer,
                    icon: const Icon(Icons.restart_alt),
                    label: const Text('Reset'),
                  ),
                ],
              ),
              const SizedBox(height: 8),
              TimerSettings(controller: controller, timer: t),
            ],
          ),
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
    return ExpansionTile(
      title: const Text('Timer Settings'),
      tilePadding: EdgeInsets.zero,
      shape: const Border(),
      collapsedShape: const Border(),
      children: [
        ListTile(
          contentPadding: EdgeInsets.zero,
          leading: const Icon(Icons.menu_book_outlined),
          title: const Text('Study time'),
          trailing: Text(_minutesSeconds(timer.workSecs)),
          onTap: () async {
            final secs = await pickDuration(
              context,
              title: 'Study time',
              initialSecs: timer.workSecs,
              limit: 'Up to 120 minutes',
            );
            if (secs != null) await controller.setTimerSettings(workSecs: secs);
          },
        ),
        ListTile(
          contentPadding: EdgeInsets.zero,
          leading: const Icon(Icons.coffee_outlined),
          title: const Text('Break time'),
          trailing: Text(_minutesSeconds(timer.breakSecs)),
          onTap: () async {
            final secs = await pickDuration(
              context,
              title: 'Break time',
              initialSecs: timer.breakSecs,
              limit: 'Up to 60 minutes',
            );
            if (secs != null) await controller.setTimerSettings(breakSecs: secs);
          },
        ),
        ListTile(
          contentPadding: EdgeInsets.zero,
          leading: const Icon(Icons.repeat),
          title: const Text('Number of loops'),
          trailing: Row(
            mainAxisSize: MainAxisSize.min,
            children: [
              IconButton(
                tooltip: 'Fewer loops',
                icon: const Icon(Icons.remove),
                onPressed: timer.loops > 1 ? () => controller.setTimerSettings(loops: timer.loops - 1) : null,
              ),
              Text('${timer.loops}'),
              IconButton(
                tooltip: 'More loops',
                icon: const Icon(Icons.add),
                onPressed: timer.loops < 20 ? () => controller.setTimerSettings(loops: timer.loops + 1) : null,
              ),
            ],
          ),
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
