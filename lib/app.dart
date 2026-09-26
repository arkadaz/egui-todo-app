import 'dart:async';

import 'package:flutter/material.dart';

import 'controller.dart';
import 'pages/focus_page.dart';
import 'pages/rewards_page.dart';
import 'pages/stats_page.dart';
import 'pages/tasks_page.dart';
import 'src/rust/api/focus_hub.dart';

/// Colors from the orange of the app icon. Dark like the desktop app, or light when the
/// phone is set to light mode. Made once: working out a palette from a seed color is slow.
ThemeData focusHubTheme(Brightness brightness) =>
    brightness == Brightness.dark ? _darkTheme : _lightTheme;

final _lightTheme = _theme(Brightness.light);
final _darkTheme = _theme(Brightness.dark);

ThemeData _theme(Brightness brightness) => ThemeData(
      useMaterial3: true,
      colorScheme: ColorScheme.fromSeed(
        seedColor: const Color(0xFFE8914A),
        brightness: brightness,
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
      theme: focusHubTheme(Brightness.light),
      darkTheme: focusHubTheme(Brightness.dark),
      themeMode: ThemeMode.system, // follow the phone's setting
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
  // Made once, so switching tabs doesn't rebuild every page.
  late final List<Widget> _pages = [
    FocusPage(controller: _controller),
    TasksPage(controller: _controller),
    StatsPage(controller: _controller),
    RewardsPage(controller: _controller),
  ];
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
    WidgetsBinding.instance.addPostFrameCallback((_) => _offerHomeIcon());
  }

  /// On first launch: "Put Focus Hub on your Home screen?" Asked once, whatever the answer.
  Future<void> _offerHomeIcon() async {
    if (!await _controller.shouldOfferHomeIcon() || !mounted) return;
    _controller.homeIconOffered();
    final add = await showDialog<bool>(
      context: context,
      builder: (context) => AlertDialog(
        icon: const Icon(Icons.add_to_home_screen),
        title: const Text('Add to Home screen?'),
        content: const Text('Put the Focus Hub icon on your Home screen, to start a session in one tap.'),
        actions: [
          TextButton(onPressed: () => Navigator.pop(context, false), child: const Text('Not now')),
          FilledButton(onPressed: () => Navigator.pop(context, true), child: const Text('Add icon')),
        ],
      ),
    );
    if (add == true) await _controller.homeScreen.add(); // Android asks the user to confirm
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

  static const _destinations = [
    (icon: Icons.timer_outlined, selectedIcon: Icons.timer, label: 'Focus'),
    (icon: Icons.checklist, selectedIcon: Icons.checklist, label: 'Tasks'),
    (icon: Icons.bar_chart, selectedIcon: Icons.bar_chart, label: 'Stats'),
    (icon: Icons.emoji_events_outlined, selectedIcon: Icons.emoji_events, label: 'Rewards'),
  ];

  @override
  Widget build(BuildContext context) {
    // IndexedStack keeps every tab alive, so text boxes and scroll positions stay put.
    // TickerMode pauses the hidden ones' animations (the background GIF would otherwise keep
    // playing, and redrawing the screen, behind the other tabs).
    final pages = IndexedStack(
      index: _page,
      children: [
        for (var i = 0; i < _pages.length; i++) TickerMode(enabled: i == _page, child: _pages[i]),
      ],
    );
    void select(int page) => setState(() => _page = page);

    // Wide windows (desktop, tablets) get a rail on the left instead of a bottom bar.
    if (MediaQuery.sizeOf(context).width >= 800) {
      return Scaffold(
        body: Row(
          children: [
            NavigationRail(
              selectedIndex: _page,
              onDestinationSelected: select,
              labelType: NavigationRailLabelType.all,
              destinations: [
                for (final d in _destinations)
                  NavigationRailDestination(
                    icon: Icon(d.icon),
                    selectedIcon: Icon(d.selectedIcon),
                    label: Text(d.label),
                  ),
              ],
            ),
            const VerticalDivider(width: 1),
            Expanded(child: pages),
          ],
        ),
      );
    }
    return Scaffold(
      body: pages,
      bottomNavigationBar: NavigationBar(
        selectedIndex: _page,
        onDestinationSelected: select,
        destinations: [
          for (final d in _destinations)
            NavigationDestination(icon: Icon(d.icon), selectedIcon: Icon(d.selectedIcon), label: d.label),
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
      theme: focusHubTheme(Brightness.dark),
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
