package com.dangu.gallery.data

import android.net.Uri

/** MediaStore 한 줄. 목록과 격자가 쓰는 값은 전부 여기 있고, EXIF는 [MediaDetails]에서 따로 읽는다. */
data class MediaItem(
    val id: Long,
    val uri: Uri,
    val name: String,
    val mimeType: String,
    val size: Long,
    val width: Int,
    val height: Int,
    /** 찍은 시각(ms). 없으면 수정 시각. */
    val date: Long,
    val modified: Long,
    val bucketId: Long,
    val bucketName: String,
    val relativePath: String,
    val durationMs: Long,
    val isVideo: Boolean,
) {
    val extension: String
        get() = name.substringAfterLast('.', "").uppercase().ifEmpty {
            mimeType.substringAfter('/').uppercase()
        }

    val resolution: String?
        get() = if (width > 0 && height > 0) "$width × $height" else null

    val megapixels: String?
        get() = if (width > 0 && height > 0 && !isVideo) {
            val mp = width.toLong() * height / 1_000_000.0
            if (mp >= 1) String.format("%.1fMP", mp) else null
        } else null
}

data class Album(
    val id: Long,
    val name: String,
    val cover: MediaItem,
    val count: Int,
)
