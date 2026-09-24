package com.dangu.gallery.ui

import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
import android.content.Intent
import android.net.Uri
import android.widget.Toast
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.CameraAlt
import androidx.compose.material.icons.outlined.CalendarMonth
import androidx.compose.material.icons.outlined.Folder
import androidx.compose.material.icons.outlined.Image
import androidx.compose.material.icons.outlined.LocationOn
import androidx.compose.material.icons.outlined.Tune
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.produceState
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.dangu.gallery.data.MediaDetails
import com.dangu.gallery.data.MediaItem
import com.dangu.gallery.data.MediaRepository

/** 사진 한 장의 정보 전부. 뷰어의 시트와 떠 있는 정보 패널이 같이 쓴다. */
@Composable
fun InfoPanel(item: MediaItem, modifier: Modifier = Modifier) {
    val context = LocalContext.current
    val repo = remember(context) { MediaRepository.get(context) }
    val details by produceState<MediaDetails?>(repo.cachedDetails(item.uri), item.uri) {
        value = repo.details(item, withAddress = true)
    }

    Column(modifier, verticalArrangement = Arrangement.spacedBy(12.dp)) {
        Text(
            item.name,
            style = MaterialTheme.typography.titleMedium,
            fontWeight = FontWeight.SemiBold,
            modifier = Modifier.clickable { copy(context, "파일 이름", item.name) },
        )

        val date = details?.dateTaken?.let(Format::exifDate)
            ?: item.date.takeIf { it > 0 }?.let(Format::fullDate)
        if (date != null) {
            Section(Icons.Outlined.CalendarMonth, "날짜", date)
        }

        Section(
            Icons.Outlined.Image, "파일",
            listOfNotNull(
                item.resolution?.let { r -> item.megapixels?.let { "$r ($it)" } ?: r },
                Format.size(context, item.size).takeIf { item.size > 0 },
                item.mimeType.ifEmpty { null },
                if (item.isVideo && item.durationMs > 0) "길이 " + Format.duration(item.durationMs) else null,
            ).joinToString("\n"),
        )

        val folder = item.relativePath.trimEnd('/').ifEmpty { item.bucketName }
        if (folder.isNotEmpty()) {
            val path = "$folder/${item.name}"
            Section(Icons.Outlined.Folder, "폴더", path, onClick = { copy(context, "경로", path) })
        }
        if (item.modified > 0 && item.modified != item.date) {
            Section(Icons.Outlined.CalendarMonth, "수정한 날짜", Format.fullDate(item.modified))
        }

        val d = details
        when {
            d == null && !item.isVideo -> Row(verticalAlignment = Alignment.CenterVertically) {
                CircularProgressIndicator(Modifier.size(16.dp), strokeWidth = 2.dp)
                Spacer(Modifier.width(8.dp))
                Text("EXIF 읽는 중…", style = MaterialTheme.typography.bodySmall)
            }
            d != null -> {
                d.camera?.let { camera ->
                    Section(
                        Icons.Outlined.CameraAlt, "카메라",
                        listOfNotNull(camera, d.lens, d.software?.let { "소프트웨어: $it" }).joinToString("\n"),
                    )
                }
                val shooting = listOfNotNull(
                    d.shootingSummary,
                    d.focalLength35?.let { "35mm 환산 ${it}mm" },
                    d.flash?.let { if (it) "플래시 사용" else "플래시 없음" },
                    d.whiteBalance?.let { "화이트밸런스 $it" },
                    d.orientation?.let { "회전 ${it}°" },
                ).joinToString("\n")
                if (shooting.isNotEmpty()) Section(Icons.Outlined.Tune, "촬영 설정", shooting)

                if (d.hasLocation) {
                    val coords = "%.6f, %.6f".format(d.latitude, d.longitude)
                    Section(
                        Icons.Outlined.LocationOn, "촬영 위치",
                        listOfNotNull(d.address, coords, d.altitude?.let { "고도 %.0fm".format(it) })
                            .joinToString("\n"),
                        action = "지도에서 보기",
                        onClick = {
                            val geo = Uri.parse("geo:${d.latitude},${d.longitude}?q=${d.latitude},${d.longitude}")
                            runCatching {
                                context.startActivity(Intent(Intent.ACTION_VIEW, geo).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
                            }
                        },
                    )
                } else if (!item.isVideo && d.camera != null) {
                    Text(
                        "위치 정보 없음",
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                }
            }
        }
    }
}

@Composable
private fun Section(
    icon: ImageVector,
    title: String,
    body: String,
    action: String? = null,
    onClick: (() -> Unit)? = null,
) {
    if (body.isBlank()) return
    Surface(
        shape = RoundedCornerShape(16.dp),
        color = MaterialTheme.colorScheme.surfaceContainerHigh,
        modifier = Modifier.fillMaxWidth().then(if (onClick != null) Modifier.clickable(onClick = onClick) else Modifier),
    ) {
        Row(Modifier.padding(14.dp), verticalAlignment = Alignment.Top) {
            Icon(icon, null, Modifier.size(20.dp), tint = MaterialTheme.colorScheme.primary)
            Spacer(Modifier.width(12.dp))
            Column(Modifier.weight(1f)) {
                Text(title, style = MaterialTheme.typography.labelMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
                Text(body, style = MaterialTheme.typography.bodyMedium)
                if (action != null) {
                    Text(action, style = MaterialTheme.typography.labelMedium, color = MaterialTheme.colorScheme.primary,
                        modifier = Modifier.padding(top = 4.dp))
                }
            }
        }
    }
}

/** 목록 한 줄에 들어가는 짧은 EXIF 요약. 스크롤하며 보이는 것만 읽는다. */
@Composable
fun ExifLine(item: MediaItem, modifier: Modifier = Modifier) {
    if (item.isVideo) return
    val context = LocalContext.current
    val repo = remember(context) { MediaRepository.get(context) }
    val details by produceState(repo.cachedDetails(item.uri), item.uri) {
        value = repo.details(item)
    }
    val d = details ?: return
    val text = listOfNotNull(d.camera, d.shootingSummary, if (d.hasLocation) "📍" else null).joinToString(" · ")
    if (text.isEmpty()) return
    Text(
        text,
        style = MaterialTheme.typography.bodySmall,
        color = MaterialTheme.colorScheme.onSurfaceVariant,
        maxLines = 1,
        overflow = TextOverflow.Ellipsis,
        modifier = modifier,
    )
}

private fun copy(context: Context, label: String, text: String) {
    val cm = context.getSystemService(ClipboardManager::class.java) ?: return
    cm.setPrimaryClip(ClipData.newPlainText(label, text))
    Toast.makeText(context, "$label 복사됨", Toast.LENGTH_SHORT).show()
}
