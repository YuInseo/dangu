package com.dangu.lumen

import android.Manifest
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.webkit.JavascriptInterface
import androidx.core.app.NotificationCompat
import androidx.core.app.NotificationManagerCompat
import androidx.core.content.ContextCompat

/**
 * 페이지 → 앱. 지금은 알림 하나뿐이다.
 * 디스코드가 아닌 페이지가 떠 있을 때는 아무것도 하지 않는다.
 */
class LumenBridge(context: Context, private val isTrusted: () -> Boolean) {
    private val appContext = context.applicationContext
    private val manager = NotificationManagerCompat.from(appContext)

    init {
        val channel = NotificationChannel(CHANNEL, "메시지", NotificationManager.IMPORTANCE_HIGH).apply {
            description = "새 메시지와 멘션"
        }
        appContext.getSystemService(NotificationManager::class.java).createNotificationChannel(channel)
    }

    @JavascriptInterface
    fun notify(title: String, body: String, tag: String) {
        if (!isTrusted()) return
        if (ContextCompat.checkSelfPermission(appContext, Manifest.permission.POST_NOTIFICATIONS)
            != PackageManager.PERMISSION_GRANTED
        ) return

        val open = PendingIntent.getActivity(
            appContext, 0,
            Intent(appContext, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_SINGLE_TOP),
            PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
        )
        val n = NotificationCompat.Builder(appContext, CHANNEL)
            .setSmallIcon(R.drawable.ic_notification)
            .setContentTitle(title.take(200))
            .setContentText(body.take(500))
            .setStyle(NotificationCompat.BigTextStyle().bigText(body.take(2000)))
            .setAutoCancel(true)
            .setContentIntent(open)
            .setGroup("messages")
            .setPriority(NotificationCompat.PRIORITY_HIGH)
            .build()
        val id = if (tag.isNotEmpty()) tag.hashCode() else (title + body).hashCode()
        runCatching { manager.notify(id, n) }
    }

    companion object {
        const val CHANNEL = "messages"
    }
}
