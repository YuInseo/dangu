package com.dangu.gallery.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyRow
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.Close
import androidx.compose.material.icons.outlined.MyLocation
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.SmallFloatingActionButton
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.produceState
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.viewinterop.AndroidView
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.LifecycleEventObserver
import androidx.lifecycle.compose.LocalLifecycleOwner
import coil.compose.AsyncImage
import com.dangu.gallery.data.GeoPhoto
import com.dangu.gallery.data.MediaItem
import com.dangu.gallery.data.MediaRepository
import org.osmdroid.config.Configuration
import org.osmdroid.tileprovider.tilesource.TileSourceFactory
import org.osmdroid.util.BoundingBox
import org.osmdroid.util.GeoPoint
import org.osmdroid.views.CustomZoomButtonsController
import org.osmdroid.views.MapView
import org.osmdroid.views.overlay.TilesOverlay
import java.io.File

/**
 * 지도 탭. 사진을 찍은 곳마다 썸네일 표식이 붙고, 누르면 그 자리의 사진들이 아래에 뜬다.
 * 지도는 OpenStreetMap이라 키가 필요 없다.
 */
@Composable
fun MapScreen(
    geo: List<GeoPhoto>,
    scan: Pair<Int, Int>?,
    onOpen: (List<MediaItem>, Int) -> Unit,
    bottomPadding: PaddingValues,
) {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    val dark = isSystemInDarkTheme()
    var selected by remember { mutableStateOf<List<GeoPhoto>?>(null) }

    val photoMap = remember {
        Configuration.getInstance().apply {
            userAgentValue = context.packageName
            val base = File(context.cacheDir, "osmdroid")
            osmdroidBasePath = base
            osmdroidTileCache = File(base, "tiles")
        }
        val map = MapView(context).apply {
            setTileSource(TileSourceFactory.MAPNIK)
            setMultiTouchControls(true)
            zoomController.setVisibility(CustomZoomButtonsController.Visibility.NEVER)
            minZoomLevel = 3.0
            maxZoomLevel = 19.0
            isVerticalMapRepetitionEnabled = false
            controller.setZoom(7.0)
            controller.setCenter(GeoPoint(36.5, 127.8)) // 사진이 들어오기 전엔 한반도
            if (dark) overlayManager.tilesOverlay.setColorFilter(TilesOverlay.INVERT_COLORS)
        }
        PhotoMapController(context, map, scope) { selected = it }
    }

    LaunchedEffect(geo) { photoMap.setPhotos(geo) }

    val lifecycle = LocalLifecycleOwner.current.lifecycle
    DisposableEffect(lifecycle) {
        val observer = LifecycleEventObserver { _, event ->
            when (event) {
                Lifecycle.Event.ON_RESUME -> photoMap.map.onResume()
                Lifecycle.Event.ON_PAUSE -> photoMap.map.onPause()
                else -> Unit
            }
        }
        lifecycle.addObserver(observer)
        photoMap.map.onResume()
        onDispose {
            lifecycle.removeObserver(observer)
            photoMap.map.onPause()
            photoMap.map.onDetach()
        }
    }

    Box(Modifier.fillMaxSize()) {
        AndroidView(factory = { photoMap.map }, modifier = Modifier.fillMaxSize())

        // 위: 제목과 진행 상황
        Surface(
            shape = RoundedCornerShape(20.dp),
            color = MaterialTheme.colorScheme.surfaceContainerHigh.copy(alpha = 0.94f),
            modifier = Modifier
                .align(Alignment.TopCenter)
                .statusBarsPadding()
                .padding(12.dp)
                .fillMaxWidth(),
        ) {
            Column(Modifier.padding(horizontal = 16.dp, vertical = 10.dp)) {
                Text("지도", style = MaterialTheme.typography.titleLarge, fontWeight = FontWeight.Bold)
                Text(
                    if (geo.isEmpty() && scan == null) "위치 정보가 있는 사진이 없습니다" else "위치가 있는 사진 ${geo.size}장",
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
                if (scan != null) {
                    Spacer(Modifier.size(6.dp))
                    LinearProgressIndicator(
                        progress = { if (scan.second == 0) 0f else scan.first / scan.second.toFloat() },
                        modifier = Modifier.fillMaxWidth(),
                    )
                    Text(
                        "사진 위치 읽는 중 ${scan.first} / ${scan.second}",
                        style = MaterialTheme.typography.labelSmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                }
            }
        }

        Column(
            Modifier
                .align(Alignment.BottomCenter)
                .padding(bottomPadding)
                .padding(horizontal = 12.dp),
            horizontalAlignment = Alignment.End,
        ) {
            if (geo.isNotEmpty()) {
                SmallFloatingActionButton(
                    onClick = {
                        val box = BoundingBox.fromGeoPointsSafe(geo.map { GeoPoint(it.lat, it.lng) })
                        runCatching { photoMap.map.zoomToBoundingBox(box.increaseByScale(1.3f), true) }
                    },
                    modifier = Modifier.padding(bottom = 8.dp),
                ) { Icon(Icons.Outlined.MyLocation, "전체 보기") }
            }
            selected?.let { group ->
                ClusterCard(group, onClose = { selected = null }, onOpen = onOpen)
            }
        }
    }
}

@Composable
private fun ClusterCard(group: List<GeoPhoto>, onClose: () -> Unit, onOpen: (List<MediaItem>, Int) -> Unit) {
    val context = LocalContext.current
    val repo = remember(context) { MediaRepository.get(context) }
    val lead = group.first()
    // 장소 이름은 대표 사진의 주소로
    val place by produceState<String?>(null, lead.item.uri) {
        value = repo.details(lead.item, withAddress = true).address
    }
    val items = remember(group) { group.map { it.item } }

    Surface(
        shape = RoundedCornerShape(24.dp),
        color = MaterialTheme.colorScheme.surfaceContainerHigh,
        shadowElevation = 8.dp,
        modifier = Modifier.fillMaxWidth(),
    ) {
        Column(Modifier.padding(vertical = 10.dp)) {
            Row(Modifier.padding(start = 16.dp, end = 4.dp), verticalAlignment = Alignment.CenterVertically) {
                Column(Modifier.weight(1f)) {
                    Text(
                        place ?: "%.4f, %.4f".format(lead.lat, lead.lng),
                        style = MaterialTheme.typography.titleSmall,
                        fontWeight = FontWeight.SemiBold,
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                    )
                    val newest = items.first().date
                    val oldest = items.last().date
                    Text(
                        "${items.size}장 · " + if (Format.localDate(newest) == Format.localDate(oldest))
                            Format.dayHeader(Format.localDate(newest))
                        else "${Format.dayHeader(Format.localDate(oldest))} ~ ${Format.dayHeader(Format.localDate(newest))}",
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                }
                IconButton(onClick = onClose) { Icon(Icons.Outlined.Close, "닫기") }
            }
            LazyRow(
                contentPadding = PaddingValues(horizontal = 12.dp, vertical = 6.dp),
                horizontalArrangement = Arrangement.spacedBy(6.dp),
            ) {
                itemsIndexed(items, key = { _, it -> it.uri.toString() }) { i, item ->
                    AsyncImage(
                        model = item.uri,
                        contentDescription = item.name,
                        contentScale = ContentScale.Crop,
                        modifier = Modifier
                            .size(84.dp)
                            .clip(RoundedCornerShape(12.dp))
                            .background(MaterialTheme.colorScheme.surfaceContainer)
                            .clickable { onOpen(items, i) },
                    )
                }
            }
        }
    }
}
