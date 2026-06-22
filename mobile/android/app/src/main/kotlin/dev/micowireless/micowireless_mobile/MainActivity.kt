package dev.micowireless.micowireless_mobile

import io.flutter.embedding.android.FlutterActivity
import io.flutter.embedding.engine.FlutterEngine
import io.flutter.plugin.common.MethodChannel

class MainActivity : FlutterActivity() {
    override fun configureFlutterEngine(flutterEngine: FlutterEngine) {
        super.configureFlutterEngine(flutterEngine)
        MethodChannel(
            flutterEngine.dartExecutor.binaryMessenger,
            "dev.micowireless.micowireless_mobile/stream_service",
        ).setMethodCallHandler { call, result ->
            when (call.method) {
                "start" -> {
                    try {
                        StreamForegroundService.start(this)
                        result.success(null)
                    } catch (error: Exception) {
                        result.error(
                            "STREAM_SERVICE_START_FAILED",
                            error.message ?: "Could not start foreground service.",
                            null,
                        )
                    }
                }

                "stop" -> {
                    try {
                        StreamForegroundService.stop(this)
                        result.success(null)
                    } catch (error: Exception) {
                        result.error(
                            "STREAM_SERVICE_STOP_FAILED",
                            error.message ?: "Could not stop foreground service.",
                            null,
                        )
                    }
                }

                else -> result.notImplemented()
            }
        }
    }
}
