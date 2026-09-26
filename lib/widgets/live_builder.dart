import 'package:flutter/widgets.dart';

import '../controller.dart';

/// Rebuilds when the data changes, and every second while its tab is on screen.
///
/// The tabs stay alive in an IndexedStack, so a hidden tab would otherwise keep redrawing
/// the timer or the stats every second for nobody. While hidden it only follows data
/// changes, and it catches up the moment it's shown again.
class LiveBuilder extends StatelessWidget {
  const LiveBuilder({super.key, required this.controller, required this.builder});

  final FocusController controller;
  final WidgetBuilder builder;

  @override
  Widget build(BuildContext context) {
    return ListenableBuilder(
      listenable: Visibility.of(context) ? controller.live : controller,
      builder: (context, _) => builder(context),
    );
  }
}
