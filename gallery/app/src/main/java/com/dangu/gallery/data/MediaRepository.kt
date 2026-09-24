package com.dangu.gallery.data

import android.content.ContentResolver
import android.content.ContentUris
import android.content.Context
import android.database.Cursor
import android.graphics.BitmapFactory
import android.location.Geocoder
import android.net.Uri
import android.provider.MediaStore
import android.provider.OpenableColumns
import android.util.LruCache
import androidx.exifinterface.media.ExifInterface
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import java.util.Locale

class MediaRepository(context: Context) {
    private val appContext = context.applicationContext
    private val resolver: ContentResolver = appContext.contentResolver

    private val detailsCache = LruCache<Uri, MediaDetails>(500)
    private val addressCache = LruCache<String, String>(200)

    /** 기기의 사진·동영상 전부, 최신순. */
    suspend fun loadAll(limit: Int? = null): List<MediaItem> = withContext(Dispatchers.IO) {
        val collection = MediaStore.Files.getContentUri(MediaStore.VOLUME_EXTERNAL)
        val selection = "${MediaStore.Files.FileColumns.MEDIA_TYPE} IN (?, ?)"
        val args = arrayOf(
            MediaStore.Files.FileColumns.MEDIA_TYPE_IMAGE.toString(),
            MediaStore.Files.FileColumns.MEDIA_TYPE_VIDEO.toString(),
        )
        val sort = "${MediaStore.MediaColumns.DATE_MODIFIED} DESC"
        val items = ArrayList<MediaItem>()
        runCatching {
            resolver.query(collection, PROJECTION + MediaStore.Files.FileColumns.MEDIA_TYPE, selection, args, sort)?.use { c ->
                while (c.moveToNext()) {
                    items += c.toMediaItem()
                }
            }
        }
        items.sortByDescending { it.date }
        if (limit != null) items.take(limit) else items
    }

    fun albums(all: List<MediaItem>): List<Album> =
        all.groupBy { it.bucketId }
            .map { (id, list) -> Album(id, list.first().bucketName, list.first(), list.size) }
            .sortedByDescending { it.count }

    /**
     * 다른 앱이 넘겨준 URI 하나를 [MediaItem]으로. MediaStore URI면 전부 채우고,
     * 그 밖의 제공자(파일 관리자, 메신저 등)면 이름과 크기, 해상도만 채운다.
     */
    suspend fun resolve(uri: Uri): MediaItem = withContext(Dispatchers.IO) {
        if (uri.authority == MediaStore.AUTHORITY) {
            // 사진 표에는 DURATION이 없을 수 있어서, 안 되면 그것만 빼고 한 번 더.
            val fromStore = listOf(PROJECTION, PROJECTION.copyOf(PROJECTION.size - 1).requireNoNulls())
                .firstNotNullOfOrNull { projection ->
                    runCatching {
                        resolver.query(uri, projection, null, null, null)?.use { c ->
                            if (c.moveToFirst()) c.toMediaItem(overrideUri = uri) else null
                        }
                    }.getOrNull()
                }
            if (fromStore != null) return@withContext fromStore
        }

        var name = uri.lastPathSegment ?: "알 수 없음"
        var size = 0L
        runCatching {
            resolver.query(uri, arrayOf(OpenableColumns.DISPLAY_NAME, OpenableColumns.SIZE), null, null, null)
                ?.use { c ->
                    if (c.moveToFirst()) {
                        c.getColumnIndex(OpenableColumns.DISPLAY_NAME).takeIf { it >= 0 }
                            ?.let { c.getString(it) }?.let { name = it }
                        c.getColumnIndex(OpenableColumns.SIZE).takeIf { it >= 0 }
                            ?.let { if (!c.isNull(it)) size = c.getLong(it) }
                    }
                }
        }
        val mime = resolver.getType(uri) ?: "image/*"
        val isVideo = mime.startsWith("video/")
        var width = 0
        var height = 0
        if (!isVideo) {
            runCatching {
                resolver.openInputStream(uri)?.use {
                    val opts = BitmapFactory.Options().apply { inJustDecodeBounds = true }
                    BitmapFactory.decodeStream(it, null, opts)
                    width = opts.outWidth
                    height = opts.outHeight
                }
            }
        }
        MediaItem(
            id = -1, uri = uri, name = name, mimeType = mime, size = size,
            width = width, height = height, date = 0, modified = 0,
            bucketId = -1, bucketName = "", relativePath = "", durationMs = 0, isVideo = isVideo,
        )
    }

    fun cachedDetails(uri: Uri): MediaDetails? = detailsCache.get(uri)

    /** EXIF. 동영상이나 EXIF가 없는 파일은 빈 값. */
    suspend fun details(item: MediaItem, withAddress: Boolean = false): MediaDetails =
        withContext(Dispatchers.IO) {
            val base = detailsCache.get(item.uri) ?: readExif(item).also { detailsCache.put(item.uri, it) }
            if (!withAddress || !base.hasLocation || base.address != null) return@withContext base
            val address = lookupAddress(base.latitude!!, base.longitude!!)
            if (address == null) base
            else base.copy(address = address).also { detailsCache.put(item.uri, it) }
        }

    private fun readExif(item: MediaItem): MediaDetails {
        if (item.isVideo) return MediaDetails()
        // 위치는 ACCESS_MEDIA_LOCATION이 있고 원본을 달라고 해야 가려지지 않는다.
        val original = if (item.uri.authority == MediaStore.AUTHORITY) {
            runCatching { MediaStore.setRequireOriginal(item.uri) }.getOrDefault(item.uri)
        } else item.uri
        val exif = openExif(original) ?: (if (original != item.uri) openExif(item.uri) else null)
            ?: return MediaDetails()

        val latLong = runCatching { exif.latLong }.getOrNull()
        return MediaDetails(
            dateTaken = exif.getAttribute(ExifInterface.TAG_DATETIME_ORIGINAL)
                ?: exif.getAttribute(ExifInterface.TAG_DATETIME),
            make = exif.getAttribute(ExifInterface.TAG_MAKE)?.clean(),
            model = exif.getAttribute(ExifInterface.TAG_MODEL)?.clean(),
            lens = exif.getAttribute(ExifInterface.TAG_LENS_MODEL)?.clean(),
            fNumber = exif.doubleOrNull(ExifInterface.TAG_F_NUMBER),
            exposureTime = exif.doubleOrNull(ExifInterface.TAG_EXPOSURE_TIME),
            iso = exif.getAttributeInt(ExifInterface.TAG_PHOTOGRAPHIC_SENSITIVITY, 0).takeIf { it > 0 },
            focalLength = exif.doubleOrNull(ExifInterface.TAG_FOCAL_LENGTH),
            focalLength35 = exif.getAttributeInt(ExifInterface.TAG_FOCAL_LENGTH_IN_35MM_FILM, 0)
                .takeIf { it > 0 },
            flash = exif.getAttribute(ExifInterface.TAG_FLASH)?.toIntOrNull()?.let { (it and 1) == 1 },
            whiteBalance = when (exif.getAttributeInt(ExifInterface.TAG_WHITE_BALANCE, -1)) {
                0 -> "자동"
                1 -> "수동"
                else -> null
            },
            software = exif.getAttribute(ExifInterface.TAG_SOFTWARE)?.clean(),
            orientation = exif.rotationDegrees.takeIf { it != 0 },
            latitude = latLong?.getOrNull(0),
            longitude = latLong?.getOrNull(1),
            altitude = exif.getAltitude(Double.NaN).takeIf { !it.isNaN() },
        )
    }

    private fun openExif(uri: Uri): ExifInterface? = runCatching {
        resolver.openInputStream(uri)?.use { ExifInterface(it) }
    }.getOrNull()

    @Suppress("DEPRECATION")
    private fun lookupAddress(lat: Double, lng: Double): String? {
        val key = "%.4f,%.4f".format(Locale.US, lat, lng)
        addressCache.get(key)?.let { return it }
        if (!Geocoder.isPresent()) return null
        val address = runCatching {
            Geocoder(appContext, Locale.KOREA).getFromLocation(lat, lng, 1)?.firstOrNull()
        }.getOrNull() ?: return null
        val text = address.getAddressLine(0)
            ?: listOfNotNull(address.adminArea, address.locality, address.subLocality, address.thoroughfare)
                .joinToString(" ")
        return text.ifBlank { null }?.also { addressCache.put(key, it) }
    }

    private fun Cursor.toMediaItem(overrideUri: Uri? = null): MediaItem {
        val id = getLong(getColumnIndexOrThrow(MediaStore.MediaColumns._ID))
        val mime = str(MediaStore.MediaColumns.MIME_TYPE) ?: ""
        val mediaType = int(MediaStore.Files.FileColumns.MEDIA_TYPE)
        val isVideo = mediaType == MediaStore.Files.FileColumns.MEDIA_TYPE_VIDEO || mime.startsWith("video/")
        val base = if (isVideo) MediaStore.Video.Media.EXTERNAL_CONTENT_URI else MediaStore.Images.Media.EXTERNAL_CONTENT_URI
        val modified = long(MediaStore.MediaColumns.DATE_MODIFIED) * 1000
        val taken = long(MediaStore.MediaColumns.DATE_TAKEN)
        var w = int(MediaStore.MediaColumns.WIDTH)
        var h = int(MediaStore.MediaColumns.HEIGHT)
        val orientation = int(MediaStore.MediaColumns.ORIENTATION)
        if (orientation == 90 || orientation == 270) {
            val t = w; w = h; h = t
        }
        return MediaItem(
            id = id,
            uri = overrideUri ?: ContentUris.withAppendedId(base, id),
            name = str(MediaStore.MediaColumns.DISPLAY_NAME) ?: "이름 없음",
            mimeType = mime,
            size = long(MediaStore.MediaColumns.SIZE),
            width = w,
            height = h,
            date = if (taken > 0) taken else modified,
            modified = modified,
            bucketId = long(MediaStore.MediaColumns.BUCKET_ID),
            bucketName = str(MediaStore.MediaColumns.BUCKET_DISPLAY_NAME) ?: "기타",
            relativePath = str(MediaStore.MediaColumns.RELATIVE_PATH) ?: "",
            durationMs = long(MediaStore.MediaColumns.DURATION),
            isVideo = isVideo,
        )
    }

    private fun Cursor.str(col: String): String? =
        getColumnIndex(col).takeIf { it >= 0 && !isNull(it) }?.let { getString(it) }

    private fun Cursor.long(col: String): Long =
        getColumnIndex(col).takeIf { it >= 0 && !isNull(it) }?.let { getLong(it) } ?: 0L

    private fun Cursor.int(col: String): Int =
        getColumnIndex(col).takeIf { it >= 0 && !isNull(it) }?.let { getInt(it) } ?: 0

    private fun String.clean(): String? = trim().trim('\u0000').ifBlank { null }

    private fun ExifInterface.doubleOrNull(tag: String): Double? =
        getAttributeDouble(tag, Double.NaN).takeIf { !it.isNaN() && it > 0 }

    companion object {
        private val PROJECTION = arrayOf(
            MediaStore.MediaColumns._ID,
            MediaStore.MediaColumns.DISPLAY_NAME,
            MediaStore.MediaColumns.MIME_TYPE,
            MediaStore.MediaColumns.SIZE,
            MediaStore.MediaColumns.WIDTH,
            MediaStore.MediaColumns.HEIGHT,
            MediaStore.MediaColumns.ORIENTATION,
            MediaStore.MediaColumns.DATE_TAKEN,
            MediaStore.MediaColumns.DATE_MODIFIED,
            MediaStore.MediaColumns.BUCKET_ID,
            MediaStore.MediaColumns.BUCKET_DISPLAY_NAME,
            MediaStore.MediaColumns.RELATIVE_PATH,
            MediaStore.MediaColumns.DURATION,
        )

        @Volatile private var instance: MediaRepository? = null
        fun get(context: Context): MediaRepository =
            instance ?: synchronized(this) {
                instance ?: MediaRepository(context).also { instance = it }
            }
    }
}
