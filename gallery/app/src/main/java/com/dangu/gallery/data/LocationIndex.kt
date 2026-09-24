package com.dangu.gallery.data

import android.content.Context
import android.provider.MediaStore
import androidx.exifinterface.media.ExifInterface
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.async
import kotlinx.coroutines.awaitAll
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.ensureActive
import kotlinx.coroutines.withContext
import java.io.File

data class GeoPhoto(val item: MediaItem, val lat: Double, val lng: Double)

/**
 * 사진마다 EXIF의 GPS 좌표.
 *
 * MediaStore의 위도·경도 열은 Android 10부터 비어 오므로 파일을 하나하나 열어 읽어야 한다.
 * 처음 한 번은 오래 걸리니 결과를 파일에 적어 두고, 다음부터는 새로 들어온 사진만 읽는다.
 * 키는 (id, 수정 시각) — 사진을 편집하면 다시 읽는다.
 */
class LocationIndex(context: Context) {
    private val appContext = context.applicationContext
    private val file = File(appContext.filesDir, "locations.tsv")

    /** "id:modified" → 좌표, 없으면 null */
    private val known = HashMap<String, DoubleArray?>()
    private var loaded = false

    private fun key(item: MediaItem) = "${item.id}:${item.modified}"

    private fun loadFile() {
        if (loaded) return
        loaded = true
        runCatching {
            file.forEachLine { line ->
                val p = line.split('\t')
                if (p.size == 3) {
                    val lat = p[1].toDoubleOrNull()
                    val lng = p[2].toDoubleOrNull()
                    known[p[0]] = if (lat != null && lng != null) doubleArrayOf(lat, lng) else null
                }
            }
        }
    }

    private fun saveFile() {
        runCatching {
            val tmp = File(file.path + ".tmp")
            tmp.bufferedWriter().use { w ->
                for ((k, v) in known) {
                    w.write(k); w.write("\t")
                    w.write(v?.get(0)?.toString() ?: "-"); w.write("\t")
                    w.write(v?.get(1)?.toString() ?: "-"); w.write("\n")
                }
            }
            tmp.renameTo(file)
        }
    }

    /**
     * [items] 가운데 좌표가 있는 사진. 모르는 사진만 파일을 열어 읽고,
     * 읽는 동안 [onProgress]로 (읽은 수, 읽어야 할 수)를 알린다.
     */
    @OptIn(ExperimentalCoroutinesApi::class)
    suspend fun scan(
        items: List<MediaItem>,
        onProgress: (done: Int, total: Int, partial: List<GeoPhoto>) -> Unit,
    ): List<GeoPhoto> = withContext(Dispatchers.IO) {
        val photos = items.filter { !it.isVideo && it.id >= 0 }
        synchronized(this@LocationIndex) { loadFile() }
        val todo = photos.filter { synchronized(this@LocationIndex) { !known.containsKey(key(it)) } }

        fun current(): List<GeoPhoto> = synchronized(this@LocationIndex) {
            photos.mapNotNull { p -> known[key(p)]?.let { GeoPhoto(p, it[0], it[1]) } }
        }

        if (todo.isNotEmpty()) {
            onProgress(0, todo.size, current())
            val io = Dispatchers.IO.limitedParallelism(4)
            var done = 0
            todo.chunked(100).forEach { chunk ->
                ensureActive()
                val results = coroutineScope {
                    chunk.map { item -> async(io) { item to readLatLng(item) } }.awaitAll()
                }
                synchronized(this@LocationIndex) {
                    results.forEach { (item, ll) -> known[key(item)] = ll }
                }
                done += chunk.size
                onProgress(done, todo.size, current())
            }
            synchronized(this@LocationIndex) { saveFile() }
        }
        current()
    }

    private fun readLatLng(item: MediaItem): DoubleArray? {
        val resolver = appContext.contentResolver
        val original = runCatching { MediaStore.setRequireOriginal(item.uri) }.getOrDefault(item.uri)
        for (uri in listOf(original, item.uri).distinct()) {
            val ll = runCatching {
                resolver.openInputStream(uri)?.use { ExifInterface(it).latLong }
            }.getOrNull()
            if (ll != null && !(ll[0] == 0.0 && ll[1] == 0.0)) return ll
            if (ll != null) return null
        }
        return null
    }

    companion object {
        @Volatile private var instance: LocationIndex? = null
        fun get(context: Context): LocationIndex =
            instance ?: synchronized(this) {
                instance ?: LocationIndex(context).also { instance = it }
            }
    }
}
