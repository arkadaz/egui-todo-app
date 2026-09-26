import Flutter
import UIKit
import UserNotifications

@main
@objc class AppDelegate: FlutterAppDelegate, FlutterImplicitEngineDelegate {
  override func application(
    _ application: UIApplication,
    didFinishLaunchingWithOptions launchOptions: [UIApplication.LaunchOptionsKey: Any]?
  ) -> Bool {
    // Lets timer alerts show even while the app is open.
    UNUserNotificationCenter.current().delegate = self as? UNUserNotificationCenterDelegate
    return super.application(application, didFinishLaunchingWithOptions: launchOptions)
  }

  func didInitializeImplicitFlutterEngine(_ engineBridge: FlutterImplicitEngineBridge) {
    GeneratedPluginRegistrant.register(with: engineBridge.pluginRegistry)
    // Tells Dart whether Low Power Mode is on, so the app can stop its background
    // animation (lib/services/battery_saver.dart).
    if let registrar = engineBridge.pluginRegistry.registrar(forPlugin: "FocusHubLowPowerMode") {
      FlutterEventChannel(name: "focus_hub/battery_saver", binaryMessenger: registrar.messenger())
        .setStreamHandler(LowPowerModeStream())
    }
  }
}

/// Sends `true` while Low Power Mode is on, now and whenever it changes.
private class LowPowerModeStream: NSObject, FlutterStreamHandler {
  private var observer: NSObjectProtocol?

  func onListen(withArguments arguments: Any?, eventSink events: @escaping FlutterEventSink) -> FlutterError? {
    events(ProcessInfo.processInfo.isLowPowerModeEnabled)
    observer = NotificationCenter.default.addObserver(
      forName: .NSProcessInfoPowerStateDidChange, object: nil, queue: .main
    ) { _ in
      events(ProcessInfo.processInfo.isLowPowerModeEnabled)
    }
    return nil
  }

  func onCancel(withArguments arguments: Any?) -> FlutterError? {
    if let observer = observer {
      NotificationCenter.default.removeObserver(observer)
    }
    observer = nil
    return nil
  }
}
