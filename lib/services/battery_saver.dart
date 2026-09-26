import 'dart:io';

import 'package:flutter/foundation.dart';
import 'package:flutter/services.dart';

/// Whether the phone is saving battery (Android's Battery Saver, iOS's Low Power Mode),
/// updated as it changes. MainActivity.kt and AppDelegate.swift send it.
final ValueListenable<bool> batterySaver = _watchBatterySaver();

ValueListenable<bool> _watchBatterySaver() {
  final on = ValueNotifier(false);
  if (!kIsWeb && (Platform.isAndroid || Platform.isIOS)) {
    const EventChannel('focus_hub/battery_saver').receiveBroadcastStream().listen(
      (value) => on.value = value == true,
      onError: (Object _) {}, // not available: treat it as off
    );
  }
  return on;
}
