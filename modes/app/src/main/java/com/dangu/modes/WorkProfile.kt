package com.dangu.modes

import android.content.ComponentName
import android.content.Context
import android.content.pm.LauncherApps
import android.graphics.Bitmap
import android.os.Process
import android.os.UserHandle
import android.os.UserManager
import com.dangu.modes.Apps.toBitmap
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

data class WorkApp(val component: ComponentName, val user: UserHandle, val label: String, val icon: Bitmap?)

/** 업무 프로필(같은 기기의 다른 사용자 프로필) 앱. 없으면 빈 목록. */
object WorkProfile {
    fun profiles(context: Context): List<UserHandle> =
        context.getSystemService(UserManager::class.java).userProfiles.filter { it != Process.myUserHandle() }

    suspend fun apps(context: Context): List<WorkApp> = withContext(Dispatchers.IO) {
        val la = context.getSystemService(LauncherApps::class.java)
        profiles(context).flatMap { user ->
            runCatching { la.getActivityList(null, user) }.getOrDefault(emptyList()).map {
                WorkApp(it.componentName, user, it.label.toString(), runCatching { it.getBadgedIcon(0).toBitmap(96) }.getOrNull())
            }
        }.sortedBy { it.label.lowercase() }
    }

    fun launch(context: Context, app: WorkApp): Boolean = runCatching {
        context.getSystemService(LauncherApps::class.java).startMainActivity(app.component, app.user, null, null)
    }.isSuccess
}
