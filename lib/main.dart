import 'package:flutter/material.dart';
import 'package:flutter_rust_bridge/flutter_rust_bridge.dart';
import 'package:path_provider/path_provider.dart';

import 'app.dart';
import 'controller.dart';
import 'services/alerts.dart';
import 'src/rust/api/focus_hub.dart';
import 'src/rust/frb_generated.dart';

Future<void> main() async {
  WidgetsFlutterBinding.ensureInitialized(); // needed before using plugins in main()
  debugPrint('FocusHub startup: loading Rust');
  await RustLib.init(); // load the Rust library

  debugPrint('FocusHub startup: finding the data folder');
  // The app's private folder. Rust keeps focushub_data.json here.
  final dataDir = await getApplicationSupportDirectory();

  debugPrint('FocusHub startup: opening ${dataDir.path}');
  final FocusHub hub;
  try {
    hub = FocusHub.open(dataDir: dataDir.path);
  } on AnyhowException catch (e) {
    runApp(StartupErrorApp(message: e.message));
    return;
  }

  debugPrint('FocusHub startup: setting up notifications');
  final alerts = Alerts();
  await alerts.init();

  debugPrint('FocusHub startup: ready');
  runApp(
    FocusHubApp(
      controller: FocusController(hub: hub, alerts: alerts),
    ),
  );
}
