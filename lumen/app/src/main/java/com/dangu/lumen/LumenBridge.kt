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
class LumenBridge(
    context: Context,
    private val isTrusted: () -> Boolean,
    private val onState: (String) -> Unit = {},
    private val onMessages: (String) -> Unit = {},
) {
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

    private val main = android.os.Handler(android.os.Looper.getMainLooper())

    /** 페이지의 마이크가 켜지거나 꺼질 때. 통화 서비스를 띄우거나 내린다. */
    @JavascriptInterface
    fun callState(active: Boolean) {
        if (active && !isTrusted()) return
        main.post { CallService.set(appContext, active) }
    }

    /** 클래식 서랍용 상태(classic.js). */
    @JavascriptInterface
    fun state(json: String) {
        if (!isTrusted()) return
        main.post { onState(json) }
    }

    /** 지금 채널의 메시지(classic.js). */
    @JavascriptInterface
    fun messages(json: String) {
        if (!isTrusted()) return
        main.post { onMessages(json) }
    }

    companion object {
        const val CHANNEL = "messages"
    }
}
