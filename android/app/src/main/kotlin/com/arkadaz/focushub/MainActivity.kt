package com.arkadaz.focushub

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.content.pm.ShortcutInfo
import android.content.pm.ShortcutManager
import android.graphics.drawable.Icon
import android.os.Build
import android.os.PowerManager
import io.flutter.embedding.android.FlutterActivity
import io.flutter.embedding.engine.FlutterEngine
import io.flutter.plugin.common.EventChannel
import io.flutter.plugin.common.MethodChannel

class MainActivity : FlutterActivity() {
    override fun configureFlutterEngine(flutterEngine: FlutterEngine) {
        super.configureFlutterEngine(flutterEngine)
        val messenger = flutterEngine.dartExecutor.binaryMessenger
        // Tells Dart whether Battery Saver is on, now and whenever it changes, so the app
        // can stop its background animation (lib/services/battery_saver.dart).
        EventChannel(messenger, "focus_hub/battery_saver").setStreamHandler(BatterySaverStream(this))
        // Puts the app's icon on the Home screen (lib/services/home_screen.dart).
        MethodChannel(messenger, "focus_hub/home_screen").setMethodCallHandler { call, result ->
            when (call.method) {
                "canAdd" -> result.success(HomeScreenIcon.canAdd(this))
                "add" -> result.success(HomeScreenIcon.add(this))
                else -> result.notImplemented()
            }
        }
    }
}

private class BatterySaverStream(private val context: Context) : EventChannel.StreamHandler {
    private var receiver: BroadcastReceiver? = null

    override fun onListen(arguments: Any?, events: EventChannel.EventSink) {
        val power = context.getSystemService(Context.POWER_SERVICE) as PowerManager
        events.success(power.isPowerSaveMode)
        val changed = object : BroadcastReceiver() {
            override fun onReceive(context: Context, intent: Intent) {
                events.success(power.isPowerSaveMode)
            }
        }
        val filter = IntentFilter(PowerManager.ACTION_POWER_SAVE_MODE_CHANGED)
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
            context.registerReceiver(changed, filter, Context.RECEIVER_NOT_EXPORTED)
        } else {
            context.registerReceiver(changed, filter)
        }
        receiver = changed
    }

    override fun onCancel(arguments: Any?) {
        receiver?.let { context.unregisterReceiver(it) }
        receiver = null
    }
}

/// Android doesn't let apps put icons on the Home screen by themselves. An app can ask the
/// launcher, which shows the user an "Add to Home screen" confirmation (Android 8 and up).
private object HomeScreenIcon {
    fun canAdd(context: Context): Boolean =
        Build.VERSION.SDK_INT >= Build.VERSION_CODES.O &&
            context.getSystemService(ShortcutManager::class.java).isRequestPinShortcutSupported

    /// Returns false if this launcher can't do it.
    fun add(context: Context): Boolean {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.O) return false
        val shortcuts = context.getSystemService(ShortcutManager::class.java)
        if (!shortcuts.isRequestPinShortcutSupported) return false
        val open = Intent(context, MainActivity::class.java).setAction(Intent.ACTION_MAIN)
        val shortcut = ShortcutInfo.Builder(context, "focus_hub")
            .setShortLabel("Focus Hub")
            .setIcon(Icon.createWithResource(context, R.mipmap.ic_launcher))
            .setIntent(open)
            .build()
        return shortcuts.requestPinShortcut(shortcut, null)
    }
}
