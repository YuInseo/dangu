package com.dangu.modes

import android.content.Context
import android.content.Intent
import android.graphics.Bitmap
import android.graphics.Canvas
import android.graphics.drawable.BitmapDrawable
import android.graphics.drawable.Drawable
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

data class AppInfo(val packageName: String, val label: String, val icon: Bitmap?)

/** 실행할 수 있는 앱 목록(매니페스트의 <queries> 덕에 보인다). */
object Apps {
    suspend fun launchable(context: Context): List<AppInfo> = withContext(Dispatchers.IO) {
        val pm = context.packageManager
        val intent = Intent(Intent.ACTION_MAIN).addCategory(Intent.CATEGORY_LAUNCHER)
        pm.queryIntentActivities(intent, 0)
            .map { it.activityInfo }
            .filter { it.packageName != context.packageName }
            .distinctBy { it.packageName }
            .map { AppInfo(it.packageName, it.loadLabel(pm).toString(), it.loadIcon(pm).toBitmap(96)) }
            .sortedBy { it.label.lowercase() }
    }

    fun label(context: Context, pkg: String): String =
        runCatching { context.packageManager.run { getApplicationLabel(getApplicationInfo(pkg, 0)).toString() } }.getOrDefault(pkg)

    fun Drawable.toBitmap(size: Int): Bitmap {
        if (this is BitmapDrawable && bitmap != null) return Bitmap.createScaledBitmap(bitmap, size, size, true)
        val b = Bitmap.createBitmap(size, size, Bitmap.Config.ARGB_8888)
        val c = Canvas(b)
        setBounds(0, 0, size, size)
        draw(c)
        return b
    }
}
