import 'dart:io';

import 'package:flutter/material.dart';

import '../app.dart' show focusHubTheme;
import '../controller.dart';
import '../services/battery_saver.dart';
import '../widgets/live_builder.dart';
import '../widgets/settings_sheet.dart';
import '../widgets/timer_settings.dart';

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
            // app's only animation, so it stops when it's turned off in Settings, when the
            // phone saves battery, or when the system asks for less motion (Image checks
            // that itself).
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
    const fallback = Image(image: AssetImage('assets/background.gif'), fit: BoxFit.contain, gaplessPlayback: true);
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
      decoration: BoxDecoration(color: const Color.fromARGB(180, 20, 20, 20), borderRadius: BorderRadius.circular(16)),
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
            Text('${t.modeLabel} ${t.loopLabel}', style: theme.textTheme.titleMedium?.copyWith(color: modeColor)),
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
