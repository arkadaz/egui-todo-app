import 'dart:io';

import 'package:flutter/foundation.dart';
import 'package:flutter/services.dart';

/// Putting Focus Hub's icon on the Home screen (Android 8 and up). Apps can't do this by
/// themselves: the app asks, and Android shows the user an "Add to Home screen"
/// confirmation (MainActivity.kt does the asking).
class HomeScreen {
  static const _channel = MethodChannel('focus_hub/home_screen');

  /// Whether this phone's launcher accepts the request.
  Future<bool> canAdd() => _call('canAdd');

  /// Shows Android's confirmation. Returns false if the launcher can't add icons.
  Future<bool> add() => _call('add');

  Future<bool> _call(String method) async {
    if (kIsWeb || !Platform.isAndroid) return false;
    try {
      return await _channel.invokeMethod<bool>(method) ?? false;
    } on PlatformException {
      return false;
    } on MissingPluginException {
      return false;
    }
  }
}
