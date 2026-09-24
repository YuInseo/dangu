@file:OptIn(ExperimentalFoundationApi::class)

package com.dangu.gallery.ui

import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.background
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.grid.GridCells
import androidx.compose.foundation.lazy.grid.GridItemSpan
import androidx.compose.foundation.lazy.grid.LazyVerticalGrid
import androidx.compose.foundation.lazy.grid.rememberLazyGridState
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.PlayArrow
import androidx.compose.material.icons.outlined.Info
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import coil.compose.AsyncImage
import com.dangu.gallery.data.MediaItem
import java.time.LocalDate

/** 격자나 목록에 들어갈 한 칸: 날짜 머리글이거나 사진. */
private sealed interface Entry {
    val key: Any

    data class Header(val date: LocalDate, val count: Int) : Entry {
        override val key: Any get() = "h$date"
    }

    data class Media(val index: Int, val item: MediaItem) : Entry {
        override val key: Any get() = item.uri.toString()
    }
}

private fun entries(items: List<MediaItem>, sectioned: Boolean): List<Entry> {
    if (!sectioned) return items.mapIndexed { i, it -> Entry.Media(i, it) }
    val out = ArrayList<Entry>(items.size + 64)
    var current: LocalDate? = null
    var headerAt = -1
    var count = 0
    items.forEachIndexed { i, item ->
        val day = Format.localDate(item.date)
        if (day != current) {
            if (headerAt >= 0) out[headerAt] = Entry.Header(current!!, count)
            current = day
            headerAt = out.size
            out += Entry.Header(day, 0)
            count = 0
        }
        out += Entry.Media(i, item)
        count++
    }
    if (headerAt >= 0) out[headerAt] = Entry.Header(current!!, count)
    return out
}

/**
 * 사진 모음을 격자나 목록으로. 목록은 사진 옆에 파일 정보(해상도·크기·형식·시각·폴더·카메라)를
 * 같이 보여 준다. 줄 끝의 ⓘ는 뷰어를 거치지 않고 바로 전체 정보를 띄운다.
 */
@Composable
fun MediaBrowser(
    items: List<MediaItem>,
    mode: ViewMode,
    columns: Int,
    sectioned: Boolean,
    onOpen: (index: Int) -> Unit,
    onInfo: (MediaItem) -> Unit,
    modifier: Modifier = Modifier,
    contentPadding: PaddingValues = PaddingValues(),
    header: (@Composable () -> Unit)? = null,
) {
    val list = remember(items, sectioned) { entries(items, sectioned) }
    when (mode) {
        ViewMode.Grid -> {
            val state = rememberLazyGridState()
            LazyVerticalGrid(
                columns = GridCells.Fixed(columns),
                state = state,
                modifier = modifier,
                contentPadding = contentPadding,
                horizontalArrangement = Arrangement.spacedBy(2.dp),
                verticalArrangement = Arrangement.spacedBy(2.dp),
            ) {
                if (header != null) item(span = { GridItemSpan(maxLineSpan) }) { header() }
                items(
                    count = list.size,
                    key = { list[it].key },
                    span = { if (list[it] is Entry.Header) GridItemSpan(maxLineSpan) else GridItemSpan(1) },
                    contentType = { if (list[it] is Entry.Header) 0 else 1 },
                ) { i ->
                    when (val e = list[i]) {
                        is Entry.Header -> DayHeader(e)
                        is Entry.Media -> GridCell(e.item, onClick = { onOpen(e.index) }, onLongClick = { onInfo(e.item) })
                    }
                }
            }
        }

        ViewMode.List -> {
            val state = rememberLazyListState()
            LazyColumn(modifier = modifier, state = state, contentPadding = contentPadding) {
                if (header != null) item { header() }
                items(
                    count = list.size,
                    key = { list[it].key },
                    contentType = { if (list[it] is Entry.Header) 0 else 1 },
                ) { i ->
                    when (val e = list[i]) {
                        is Entry.Header -> DayHeader(e)
                        is Entry.Media -> ListRow(e.item, onClick = { onOpen(e.index) }, onInfo = { onInfo(e.item) })
                    }
                }
            }
        }
    }
}

@Composable
private fun DayHeader(h: Entry.Header) {
    Row(
        Modifier.fillMaxWidth().padding(start = 16.dp, end = 16.dp, top = 20.dp, bottom = 8.dp),
        verticalAlignment = Alignment.Bottom,
    ) {
        Text(Format.dayHeader(h.date), style = MaterialTheme.typography.titleSmall, fontWeight = FontWeight.Bold)
        Spacer(Modifier.width(8.dp))
        Text("${h.count}", style = MaterialTheme.typography.labelMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
    }
}

@Composable
private fun GridCell(item: MediaItem, onClick: () -> Unit, onLongClick: () -> Unit) {
    Box(
        Modifier
            .aspectRatio(1f)
            .background(MaterialTheme.colorScheme.surfaceContainer)
            .combinedClickable(onClick = onClick, onLongClick = onLongClick)
    ) {
        AsyncImage(
            model = item.uri,
            contentDescription = item.name,
            contentScale = ContentScale.Crop,
            modifier = Modifier.matchParentSize(),
        )
        if (item.isVideo) VideoBadge(item, Modifier.align(Alignment.TopEnd).padding(4.dp))
    }
}

@Composable
private fun VideoBadge(item: MediaItem, modifier: Modifier = Modifier) {
    Row(
        modifier
            .clip(RoundedCornerShape(6.dp))
            .background(Color.Black.copy(alpha = 0.55f))
            .padding(horizontal = 4.dp, vertical = 1.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Icon(Icons.Filled.PlayArrow, null, Modifier.size(12.dp), tint = Color.White)
        if (item.durationMs > 0) {
            Text(Format.duration(item.durationMs), style = MaterialTheme.typography.labelSmall, color = Color.White)
        }
    }
}

@Composable
private fun ListRow(item: MediaItem, onClick: () -> Unit, onInfo: () -> Unit) {
    val context = LocalContext.current
    Row(
        Modifier
            .fillMaxWidth()
            .combinedClickable(onClick = onClick, onLongClick = onInfo)
            .padding(start = 16.dp, end = 4.dp, top = 6.dp, bottom = 6.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Box(
            Modifier
                .size(76.dp)
                .clip(RoundedCornerShape(12.dp))
                .background(MaterialTheme.colorScheme.surfaceContainer)
        ) {
            AsyncImage(
                model = item.uri,
                contentDescription = item.name,
                contentScale = ContentScale.Crop,
                modifier = Modifier.matchParentSize(),
            )
            if (item.isVideo) VideoBadge(item, Modifier.align(Alignment.BottomEnd).padding(3.dp))
        }
        Spacer(Modifier.width(14.dp))
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
            Text(
                item.name,
                style = MaterialTheme.typography.bodyLarge,
                fontWeight = FontWeight.Medium,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
            Text(
                Format.summary(context, item),
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
            Text(
                listOfNotNull(
                    item.date.takeIf { it > 0 }?.let(Format::time),
                    item.bucketName.ifEmpty { null },
                ).joinToString(" · "),
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
            ExifLine(item)
        }
        IconButton(onClick = onInfo) {
            Icon(Icons.Outlined.Info, contentDescription = "정보", tint = MaterialTheme.colorScheme.onSurfaceVariant)
        }
    }
}
