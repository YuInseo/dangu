package com.dangu.modes

import android.Manifest
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import androidx.core.app.NotificationCompat
import androidx.core.content.ContextCompat
import androidx.core.content.edit

/**
 * 자동 업데이트의 "자동" 부분.
 *
 * 설정 화면을 열 때마다, 그리고 떠 있는 버튼 서비스가 돌 때 반나절에 한 번 새 버전을 확인하고, 있으면
 * 뒤에서 받아 둔 다음 알림을 띄운다. 알림을 누르면 설정 화면이 열리며 설치 창이 뜬다. 안드로이드는
 * 사이드로딩한 앱이 스스로를 조용히 설치하게 두지 않으므로 마지막 한 번은 사람이 누른다.
 */
object AutoUpdate {
    private const val CHANNEL = "update"
    private const val NOTIFICATION_ID = 11
    private const val INTERVAL = 12 * 60 * 60 * 1000L
    const val EXTRA_INSTALL = "install_update"

    /** 반나절이 안 지났으면 건너뛴다(force면 무조건). */
    suspend fun run(context: Context, force: Boolean = false) {
        if (BuildConfig.E2E) return
        val sp = context.getSharedPreferences("modes", Context.MODE_PRIVATE)
        val now = System.currentTimeMillis()
        if (!force && now - sp.getLong("updateCheckedAt", 0) < INTERVAL) return
        sp.edit { putLong("updateCheckedAt", now) }
        val updater = Updater.get(context)
        updater.check()
        val s = updater.state.value
        if (s is Updater.State.Ready) notifyReady(context, s.versionName)
    }

    private fun notifyReady(context: Context, version: String) {
        if (ContextCompat.checkSelfPermission(context, Manifest.permission.POST_NOTIFICATIONS) != PackageManager.PERMISSION_GRANTED) return
        val nm = context.getSystemService(NotificationManager::class.java)
        nm.createNotificationChannel(NotificationChannel(CHANNEL, "업데이트", NotificationManager.IMPORTANCE_DEFAULT))
        val open = PendingIntent.getActivity(
            context, 2,
            Intent(context, MainActivity::class.java).putExtra(EXTRA_INSTALL, true).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK),
            PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
        )
        nm.notify(
            NOTIFICATION_ID,
            NotificationCompat.Builder(context, CHANNEL)
                .setSmallIcon(R.drawable.ic_stat)
                .setContentTitle("모드 스위치 새 버전 $version")
                .setContentText("받아 두었어요. 눌러서 설치")
                .setContentIntent(open)
                .setAutoCancel(true)
                .build(),
        )
    }
}
