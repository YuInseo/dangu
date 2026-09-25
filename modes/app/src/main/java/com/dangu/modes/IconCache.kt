package com.dangu.modes

import android.content.Context
import android.util.LruCache
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.asImageBitmap
import com.dangu.modes.Apps.toBitmap
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

/** 앱 아이콘·이름 캐시. 바탕화면은 같은 아이콘을 자주 다시 그린다. */
object IconCache {
    private val icons = LruCache<String, ImageBitmap>(200)
    private val labels = LruCache<String, String>(400)

    fun cached(pkg: String): ImageBitmap? = icons.get(pkg)

    suspend fun icon(context: Context, pkg: String): ImageBitmap? {
        icons.get(pkg)?.let { return it }
        return withContext(Dispatchers.IO) {
            runCatching { context.packageManager.getApplicationIcon(pkg).toBitmap(144).asImageBitmap() }.getOrNull()
                ?.also { icons.put(pkg, it) }
        }
    }

    fun label(context: Context, pkg: String): String =
        labels.get(pkg) ?: Apps.label(context, pkg).also { labels.put(pkg, it) }
}
