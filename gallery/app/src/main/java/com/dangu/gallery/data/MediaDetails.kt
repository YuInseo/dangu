package com.dangu.gallery.data

/** EXIF에서 읽은 것. 어느 값이든 없을 수 있다. */
data class MediaDetails(
    val dateTaken: String? = null,
    val make: String? = null,
    val model: String? = null,
    val lens: String? = null,
    val fNumber: Double? = null,
    val exposureTime: Double? = null,
    val iso: Int? = null,
    val focalLength: Double? = null,
    val focalLength35: Int? = null,
    val flash: Boolean? = null,
    val whiteBalance: String? = null,
    val software: String? = null,
    val orientation: Int? = null,
    val latitude: Double? = null,
    val longitude: Double? = null,
    val altitude: Double? = null,
    val address: String? = null,
) {
    val camera: String?
        get() {
            val mk = make?.trim().orEmpty()
            val md = model?.trim().orEmpty()
            return when {
                md.isEmpty() && mk.isEmpty() -> null
                md.isEmpty() -> mk
                mk.isEmpty() || md.startsWith(mk, ignoreCase = true) -> md
                else -> "$mk $md"
            }
        }

    /** "f/1.8 · 1/120s · ISO 50 · 6.3mm" 처럼 한 줄 요약 */
    val shootingSummary: String?
        get() = listOfNotNull(
            fNumber?.let { "f/" + trimNumber(it) },
            exposureTime?.let { formatExposure(it) },
            iso?.let { "ISO $it" },
            focalLength?.let { trimNumber(it) + "mm" },
        ).joinToString(" · ").ifEmpty { null }

    val hasLocation: Boolean get() = latitude != null && longitude != null

    val isEmpty: Boolean
        get() = camera == null && shootingSummary == null && !hasLocation && dateTaken == null

    companion object {
        fun formatExposure(seconds: Double): String =
            if (seconds >= 1 || seconds <= 0) trimNumber(seconds) + "s"
            else "1/" + Math.round(1 / seconds) + "s"

        fun trimNumber(v: Double): String =
            if (v == Math.floor(v)) v.toLong().toString() else String.format("%.1f", v)
    }
}
