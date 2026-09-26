import 'dart:async';

import 'package:flutter/material.dart';

import 'controller.dart';
import 'pages/focus_page.dart';
import 'pages/rewards_page.dart';
import 'pages/stats_page.dart';
import 'pages/tasks_page.dart';
import 'src/rust/api/focus_hub.dart';

/// The desktop app's dark look, with the orange of the app icon.
ThemeData focusHubTheme() => ThemeData(
      useMaterial3: true,
      colorScheme: ColorScheme.fromSeed(
        seedColor: const Color(0xFFE8914A),
        brightness: Brightness.dark,
      ),
    );

class FocusHubApp extends StatelessWidget {
  const FocusHubApp({super.key, required this.controller});

  final FocusController controller;

  @override
  Widget build(BuildContext context) {
    return MaterialApp(
      title: 'Focus Hub',
      debugShowCheckedModeBanner: false,
      theme: focusHubTheme(),
      home: HomeShell(controller: controller),
    );
  }
}

/// The four tabs, plus everything that isn't one screen's job: ticking while the app is
/// visible, saving when it's hidden, and showing session alerts and messages.
class HomeShell extends StatefulWidget {
  const HomeShell({super.key, required this.controller});

  final FocusController controller;

  @override
  State<HomeShell> createState() => _HomeShellState();
}

class _HomeShellState extends State<HomeShell> {
  int _page = 0;
  late final AppLifecycleListener _lifecycle;
  late final StreamSubscription<SessionAlert> _alerts;
  late final StreamSubscription<String> _messages;

  FocusController get _controller => widget.controller;

  @override
  void initState() {
    super.initState();
    // Listen first: the first tick reports sessions that ended while the app was closed.
    _alerts = _controller.sessionEnded.listen(_showSessionAlert);
    _messages = _controller.messages.listen(_showMessage);
    _controller.resume();
    _lifecycle = AppLifecycleListener(
      onStateChange: (state) {
        switch (state) {
          case AppLifecycleState.resumed:
            _controller.resume();
          case AppLifecycleState.hidden || AppLifecycleState.paused || AppLifecycleState.detached:
            _controller.pause();
          case AppLifecycleState.inactive:
            break; // e.g. a permission popup is covering the app
        }
      },
    );

    final warning = _controller.hub.loadWarning();
    if (warning != null) {
      WidgetsBinding.instance.addPostFrameCallback((_) => _showWarning(warning));
    }
  }

  @override
  void dispose() {
    _lifecycle.dispose();
    _alerts.cancel();
    _messages.cancel();
    super.dispose();
  }

  void _showSessionAlert(SessionAlert alert) {
    final messenger = ScaffoldMessenger.of(context);
    messenger.hideCurrentSnackBar();
    messenger.showSnackBar(
      SnackBar(
        behavior: SnackBarBehavior.floating,
        duration: const Duration(seconds: 8),
        content: Row(
          children: [
            const Icon(Icons.alarm_on),
            const SizedBox(width: 12),
            Expanded(
              child: Column(
                mainAxisSize: MainAxisSize.min,
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Text(alert.title, style: const TextStyle(fontWeight: FontWeight.bold)),
                  Text(alert.body),
                ],
              ),
            ),
          ],
        ),
      ),
    );
  }

  void _showMessage(String message) {
    ScaffoldMessenger.of(context).showSnackBar(
      SnackBar(content: Text(message), behavior: SnackBarBehavior.floating),
    );
  }

  void _showWarning(String warning) {
    showDialog<void>(
      context: context,
      builder: (context) => AlertDialog(
        icon: const Icon(Icons.warning_amber),
        title: const Text('Saved data problem'),
        content: Text(warning),
        actions: [
          TextButton(onPressed: () => Navigator.pop(context), child: const Text('OK')),
        ],
      ),
    );
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      // IndexedStack keeps every tab alive, so text boxes and scroll positions stay put.
      body: IndexedStack(
        index: _page,
        children: [
          FocusPage(controller: _controller),
          TasksPage(controller: _controller),
          StatsPage(controller: _controller),
          RewardsPage(controller: _controller),
        ],
      ),
      bottomNavigationBar: NavigationBar(
        selectedIndex: _page,
        onDestinationSelected: (page) => setState(() => _page = page),
        destinations: const [
          NavigationDestination(icon: Icon(Icons.timer_outlined), selectedIcon: Icon(Icons.timer), label: 'Focus'),
          NavigationDestination(icon: Icon(Icons.checklist), label: 'Tasks'),
          NavigationDestination(icon: Icon(Icons.bar_chart), label: 'Stats'),
          NavigationDestination(
            icon: Icon(Icons.emoji_events_outlined),
            selectedIcon: Icon(Icons.emoji_events),
            label: 'Rewards',
          ),
        ],
      ),
    );
  }
}

/// Shown only if the saved data folder can't be opened at all.
class StartupErrorApp extends StatelessWidget {
  const StartupErrorApp({super.key, required this.message});

  final String message;

  @override
  Widget build(BuildContext context) {
    return MaterialApp(
      debugShowCheckedModeBanner: false,
      theme: focusHubTheme(),
      home: Scaffold(
        body: Center(
          child: Padding(
            padding: const EdgeInsets.all(24),
            child: Column(
              mainAxisSize: MainAxisSize.min,
              children: [
                const Icon(Icons.error_outline, size: 48),
                const SizedBox(height: 16),
                const Text('Focus Hub could not start', style: TextStyle(fontSize: 20)),
                const SizedBox(height: 8),
                Text(message, textAlign: TextAlign.center),
              ],
            ),
          ),
        ),
      ),
    );
  }
}
