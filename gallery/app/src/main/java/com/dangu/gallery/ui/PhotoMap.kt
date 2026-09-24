package com.dangu.gallery.ui

import android.content.Context
import android.graphics.Bitmap
import android.graphics.Canvas
import android.graphics.Paint
import android.graphics.Path
import android.graphics.Point
import android.graphics.RectF
import android.graphics.Typeface
import android.graphics.drawable.BitmapDrawable
import coil.imageLoader
import coil.request.ImageRequest
import coil.request.SuccessResult
import coil.size.Scale
import com.dangu.gallery.data.GeoPhoto
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import org.osmdroid.events.MapListener
import org.osmdroid.events.ScrollEvent
import org.osmdroid.events.ZoomEvent
import org.osmdroid.util.BoundingBox
import org.osmdroid.util.GeoPoint
import org.osmdroid.views.MapView
import org.osmdroid.views.overlay.Marker
import kotlin.math.roundToInt

/**
 * 지도 위의 사진 표식.
 *
 * 가까운 사진은 화면 격자 한 칸에 묶어 표식 하나로 그린다 — 가장 최근 사진의 썸네일에
 * 장 수를 붙여서. 지도를 움직이거나 확대하면 다시 묶는다.
 */
class PhotoMapController(
    private val context: Context,
    val map: MapView,
    private val scope: CoroutineScope,
    private val onClusterClick: (List<GeoPhoto>) -> Unit,
) {
    private val density = context.resources.displayMetrics.density
    private val cellPx = (72 * density).roundToInt()
    private val iconPx = (56 * density).roundToInt()

    private var photos: List<GeoPhoto> = emptyList()
    private var fitted = false
    private var reclusterJob: Job? = null
    private val thumbs = HashMap<String, Bitmap>()
    private val loading = HashSet<String>()
    private val markers = ArrayList<Marker>()

    init {
        map.addMapListener(object : MapListener {
            override fun onScroll(event: ScrollEvent?): Boolean { schedule(); return false }
            override fun onZoom(event: ZoomEvent?): Boolean { schedule(); return false }
        })
        map.addOnFirstLayoutListener { _, _, _, _, _ -> fit(); recluster() }
    }

    fun setPhotos(list: List<GeoPhoto>) {
        if (list == photos) return
        photos = list
        if (!fitted) fit()
        schedule(0)
    }

    /** 처음 한 번, 사진이 있는 곳이 다 보이게. */
    private fun fit() {
        if (fitted || photos.isEmpty() || map.width == 0) return
        fitted = true
        if (photos.size == 1) {
            map.controller.setZoom(14.0)
            map.controller.setCenter(GeoPoint(photos[0].lat, photos[0].lng))
            return
        }
        val box = BoundingBox.fromGeoPointsSafe(photos.map { GeoPoint(it.lat, it.lng) })
        runCatching { map.zoomToBoundingBox(box.increaseByScale(1.3f), false) }
    }

    private fun schedule(delayMs: Long = 150) {
        reclusterJob?.cancel()
        reclusterJob = scope.launch {
            if (delayMs > 0) delay(delayMs)
            recluster()
        }
    }

    private fun recluster() {
        if (map.width == 0) return
        val projection = map.projection
        val cells = LinkedHashMap<Long, MutableList<GeoPhoto>>()
        val pt = Point()
        val margin = cellPx
        for (p in photos) {
            projection.toPixels(GeoPoint(p.lat, p.lng), pt)
            if (pt.x < -margin || pt.y < -margin || pt.x > map.width + margin || pt.y > map.height + margin) continue
            val key = (Math.floorDiv(pt.x, cellPx).toLong() shl 32) or (Math.floorDiv(pt.y, cellPx).toLong() and 0xffffffffL)
            cells.getOrPut(key) { ArrayList() }.add(p)
        }

        map.overlays.removeAll(markers.toSet())
        markers.clear()
        for (group in cells.values) {
            val sorted = group.sortedByDescending { it.item.date }
            val lead = sorted.first()
            val marker = Marker(map).apply {
                position = GeoPoint(lead.lat, lead.lng)
                setAnchor(Marker.ANCHOR_CENTER, Marker.ANCHOR_BOTTOM)
                icon = BitmapDrawable(context.resources, badge(thumbs[lead.item.uri.toString()], sorted.size))
                setOnMarkerClickListener { _, _ -> onClusterClick(sorted); true }
            }
            markers += marker
            map.overlays.add(marker)
            loadThumb(lead)
        }
        map.invalidate()
    }

    private fun loadThumb(p: GeoPhoto) {
        val key = p.item.uri.toString()
        if (thumbs.containsKey(key) || !loading.add(key)) return
        scope.launch {
            val request = ImageRequest.Builder(context)
                .data(p.item.uri)
                .size(iconPx)
                .scale(Scale.FILL)
                .allowHardware(false)
                .build()
            val result = context.imageLoader.execute(request)
            val bmp = ((result as? SuccessResult)?.drawable as? BitmapDrawable)?.bitmap
            loading.remove(key)
            if (bmp != null) {
                thumbs[key] = bmp
                schedule(50)
            }
        }
    }

    /** 흰 테두리 둥근 사각형 썸네일 + 아래 꼬리 + 오른쪽 위 장 수. */
    private fun badge(thumb: Bitmap?, count: Int): Bitmap {
        val pad = (6 * density)
        val tail = (8 * density)
        val w = iconPx + pad * 2
        val h = iconPx + pad * 2 + tail
        val bmp = Bitmap.createBitmap(w.roundToInt(), h.roundToInt(), Bitmap.Config.ARGB_8888)
        val c = Canvas(bmp)
        val paint = Paint(Paint.ANTI_ALIAS_FLAG)
        val border = 3 * density
        val box = RectF(pad - border, pad - border, pad + iconPx + border, pad + iconPx + border)

        paint.color = 0xFF2B6BFF.toInt()
        c.drawRoundRect(box, 12 * density, 12 * density, paint)
        val tri = Path().apply {
            moveTo(w / 2 - tail, box.bottom - 1)
            lineTo(w / 2 + tail, box.bottom - 1)
            lineTo(w / 2, box.bottom + tail)
            close()
        }
        c.drawPath(tri, paint)

        val inner = RectF(pad, pad, pad + iconPx, pad + iconPx)
        if (thumb != null) {
            val save = c.save()
            c.clipPath(Path().apply { addRoundRect(inner, 9 * density, 9 * density, Path.Direction.CW) })
            // 가운데를 잘라 정사각형으로
            val s = minOf(thumb.width, thumb.height)
            val src = android.graphics.Rect((thumb.width - s) / 2, (thumb.height - s) / 2, (thumb.width + s) / 2, (thumb.height + s) / 2)
            c.drawBitmap(thumb, src, inner, Paint(Paint.FILTER_BITMAP_FLAG))
            c.restoreToCount(save)
        } else {
            paint.color = 0xFF3A3C44.toInt()
            c.drawRoundRect(inner, 9 * density, 9 * density, paint)
        }

        if (count > 1) {
            val text = if (count > 999) "999+" else count.toString()
            val tp = Paint(Paint.ANTI_ALIAS_FLAG).apply {
                color = 0xFFFFFFFF.toInt()
                textSize = 11 * density
                typeface = Typeface.DEFAULT_BOLD
                textAlign = Paint.Align.CENTER
            }
            val tw = maxOf(tp.measureText(text) + 10 * density, 20 * density)
            val r = RectF(w - tw, 0f, w.toFloat(), 20 * density)
            paint.color = 0xFFFF5A36.toInt()
            c.drawRoundRect(r, 10 * density, 10 * density, paint)
            c.drawText(text, r.centerX(), r.centerY() - (tp.ascent() + tp.descent()) / 2, tp)
        }
        return bmp
    }
}
