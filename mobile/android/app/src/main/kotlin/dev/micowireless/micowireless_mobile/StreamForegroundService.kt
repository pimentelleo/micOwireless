package dev.micowireless.micowireless_mobile

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.Context
import android.content.Intent
import android.net.wifi.WifiManager
import android.os.Build
import android.os.IBinder
import android.os.PowerManager
import androidx.core.app.NotificationCompat

class StreamForegroundService : Service() {
    private var wakeLock: PowerManager.WakeLock? = null
    private var wifiLock: WifiManager.WifiLock? = null

    override fun onBind(intent: Intent?): IBinder? = null

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        startForeground(
            NOTIFICATION_ID,
            buildNotification(),
        )
        acquireWakeLock()
        acquireWifiLock()
        return START_NOT_STICKY
    }

    override fun onDestroy() {
        releaseWakeLock()
        releaseWifiLock()
        super.onDestroy()
    }

    private fun buildNotification(): Notification {
        createChannelIfNeeded()
        val launchIntent =
            packageManager.getLaunchIntentForPackage(packageName)
                ?: Intent(this, MainActivity::class.java)
        val pendingIntentFlags =
            PendingIntent.FLAG_UPDATE_CURRENT or
                if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.M) PendingIntent.FLAG_IMMUTABLE else 0
        val pendingIntent =
            PendingIntent.getActivity(
                this,
                0,
                launchIntent,
                pendingIntentFlags,
            )

        return NotificationCompat
            .Builder(this, CHANNEL_ID)
            .setSmallIcon(R.mipmap.ic_launcher)
            .setContentTitle("micOwireless streaming")
            .setContentText("Microphone streaming stays active while screen is off.")
            .setContentIntent(pendingIntent)
            .setOngoing(true)
            .setOnlyAlertOnce(true)
            .setPriority(NotificationCompat.PRIORITY_LOW)
            .build()
    }

    private fun createChannelIfNeeded() {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.O) {
            return
        }
        val manager = getSystemService(NotificationManager::class.java)
        if (manager.getNotificationChannel(CHANNEL_ID) != null) {
            return
        }
        val channel =
            NotificationChannel(
                CHANNEL_ID,
                "micOwireless background streaming",
                NotificationManager.IMPORTANCE_LOW,
            )
        channel.description = "Keeps audio capture active while streaming in background."
        manager.createNotificationChannel(channel)
    }

    private fun acquireWakeLock() {
        val manager = getSystemService(Context.POWER_SERVICE) as? PowerManager ?: return
        if (wakeLock?.isHeld == true) {
            return
        }
        wakeLock =
            manager.newWakeLock(
                PowerManager.PARTIAL_WAKE_LOCK,
                "$packageName:StreamWakeLock",
            ).apply {
                setReferenceCounted(false)
                acquire()
            }
    }

    private fun releaseWakeLock() {
        wakeLock?.let {
            if (it.isHeld) {
                it.release()
            }
        }
        wakeLock = null
    }

    private fun acquireWifiLock() {
        val manager = applicationContext.getSystemService(Context.WIFI_SERVICE) as? WifiManager ?: return
        if (wifiLock?.isHeld == true) {
            return
        }
        val lockMode =
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
                WifiManager.WIFI_MODE_FULL_LOW_LATENCY
            } else {
                WifiManager.WIFI_MODE_FULL_HIGH_PERF
            }
        wifiLock =
            manager.createWifiLock(lockMode, "$packageName:StreamWifiLock").apply {
                setReferenceCounted(false)
                acquire()
            }
    }

    private fun releaseWifiLock() {
        wifiLock?.let {
            if (it.isHeld) {
                it.release()
            }
        }
        wifiLock = null
    }

    companion object {
        private const val CHANNEL_ID = "micowireless_streaming"
        private const val NOTIFICATION_ID = 49000

        fun start(context: Context) {
            val intent = Intent(context, StreamForegroundService::class.java)
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
                context.startForegroundService(intent)
            } else {
                context.startService(intent)
            }
        }

        fun stop(context: Context) {
            context.stopService(Intent(context, StreamForegroundService::class.java))
        }
    }
}
