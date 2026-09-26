import 'dart:io';

import 'package:flutter/foundation.dart';
import 'package:flutter/services.dart';

/// Whether Android's Battery Saver is on, updated as it changes (MainActivity.kt sends it).
/// Always off on other platforms.
final ValueListenable<bool> batterySaver = _watchBatterySaver();

ValueListenable<bool> _watchBatterySaver() {
  final on = ValueNotifier(false);
  if (!kIsWeb && Platform.isAndroid) {
    const EventChannel('focus_hub/battery_saver').receiveBroadcastStream().listen(
          (value) => on.value = value == true,
          onError: (Object _) {}, // not available: treat it as off
        );
  }
  return on;
}
