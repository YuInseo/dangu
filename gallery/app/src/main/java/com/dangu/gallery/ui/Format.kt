package com.dangu.gallery.ui

import android.content.Context
import android.text.format.Formatter
import com.dangu.gallery.data.MediaItem
import java.time.Instant
import java.time.LocalDate
import java.time.ZoneId
import java.time.format.DateTimeFormatter
import java.util.Locale

object Format {
    private val zone: ZoneId get() = ZoneId.systemDefault()
    private val dayFull = DateTimeFormatter.ofPattern("yyyy년 M월 d일 (E)", Locale.KOREAN)
    private val dayThisYear = DateTimeFormatter.ofPattern("M월 d일 (E)", Locale.KOREAN)
    private val time = DateTimeFormatter.ofPattern("a h:mm", Locale.KOREAN)
    private val full = DateTimeFormatter.ofPattern("yyyy년 M월 d일 (E) a h:mm:ss", Locale.KOREAN)

    fun localDate(ms: Long): LocalDate = Instant.ofEpochMilli(ms).atZone(zone).toLocalDate()

    fun dayHeader(date: LocalDate): String {
        val today = LocalDate.now(zone)
        return when {
            date == today -> "오늘"
            date == today.minusDays(1) -> "어제"
            date.year == today.year -> date.format(dayThisYear)
            else -> date.format(dayFull)
        }
    }

    fun time(ms: Long): String = Instant.ofEpochMilli(ms).atZone(zone).format(time)

    fun fullDate(ms: Long): String = Instant.ofEpochMilli(ms).atZone(zone).format(full)

    fun size(context: Context, bytes: Long): String =
        if (bytes <= 0) "-" else Formatter.formatFileSize(context, bytes)

    fun duration(ms: Long): String {
        val total = ms / 1000
        val h = total / 3600
        val m = (total % 3600) / 60
        val s = total % 60
        return if (h > 0) "%d:%02d:%02d".format(h, m, s) else "%d:%02d".format(m, s)
    }

    /** "4032 × 3024 · 3.2 MB · JPG" */
    fun summary(context: Context, item: MediaItem): String =
        listOfNotNull(
            item.resolution,
            size(context, item.size).takeIf { item.size > 0 },
            item.extension.takeIf { it.isNotEmpty() },
            if (item.isVideo && item.durationMs > 0) duration(item.durationMs) else null,
        ).joinToString(" · ")

    /** EXIF의 "2026:09:24 21:27:03" → "2026년 9월 24일 오후 9:27:03" */
    fun exifDate(raw: String): String {
        val m = Regex("""(\d{4}):(\d{2}):(\d{2}) (\d{2}):(\d{2}):(\d{2})""").find(raw) ?: return raw
        val (y, mo, d, h, mi, s) = m.destructured
        val hour = h.toInt()
        val ampm = if (hour < 12) "오전" else "오후"
        val h12 = if (hour % 12 == 0) 12 else hour % 12
        return "${y}년 ${mo.toInt()}월 ${d.toInt()}일 $ampm $h12:$mi:$s"
    }
}
