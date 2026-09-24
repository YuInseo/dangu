package com.dangu.gallery.ui

import androidx.activity.compose.BackHandler
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.asPaddingValues
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBars
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.grid.GridCells
import androidx.compose.foundation.lazy.grid.GridItemSpan
import androidx.compose.foundation.lazy.grid.LazyVerticalGrid
import androidx.compose.foundation.lazy.grid.items
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.automirrored.outlined.ViewList
import androidx.compose.material.icons.filled.Collections
import androidx.compose.material.icons.filled.Image
import androidx.compose.material.icons.filled.Map
import androidx.compose.material.icons.outlined.Map
import androidx.compose.material.icons.outlined.Collections
import androidx.compose.material.icons.outlined.GridView
import androidx.compose.material.icons.outlined.Image
import androidx.compose.material.icons.outlined.Remove
import androidx.compose.material.icons.outlined.Add
import androidx.compose.material.icons.automirrored.outlined.Sort
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.SegmentedButton
import androidx.compose.material3.SegmentedButtonDefaults
import androidx.compose.material3.SingleChoiceSegmentedButtonRow
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import coil.compose.AsyncImage
import com.dangu.gallery.data.Album
import com.dangu.gallery.data.MediaItem

private enum class Tab { Photos, Albums, Map }

/** 앨범 목록 맨 앞에 붙는 가상의 앨범들 */
private const val ALL_ID = Long.MIN_VALUE
private const val VIDEOS_ID = Long.MIN_VALUE + 1

@Composable
fun GalleryApp(vm: GalleryViewModel = viewModel()) {
    WithMediaPermission { access, requestAgain ->
        LaunchedEffect(access) { vm.start() }
        val state by vm.state.collectAsStateWithLifecycle()

        var tab by rememberSaveable { mutableStateOf(Tab.Photos) }
        var albumId by rememberSaveable { mutableStateOf<Long?>(null) }
        // 뷰어는 연 순간의 목록을 붙잡는다 — 그 사이 새 사진이 들어와도 보던 칸이 밀리지 않게.
        var viewer by remember { mutableStateOf<Pair<List<MediaItem>, Int>?>(null) }
        var infoFor by remember { mutableStateOf<MediaItem?>(null) }

        val allSorted = remember(state.items, state.sort) { state.items.ordered(state.sort) }
        val album: Album? = albumId?.let { id -> virtualAlbums(state.items).find { it.id == id } ?: state.albums.find { it.id == id } }
        val albumItems = remember(album, allSorted) {
            when (album?.id) {
                null -> emptyList()
                ALL_ID -> allSorted
                VIDEOS_ID -> allSorted.filter { it.isVideo }
                else -> allSorted.filter { it.bucketId == album.id }
            }
        }

        BackHandler(enabled = viewer == null && albumId != null) { albumId = null }

        Box(Modifier.fillMaxSize().background(MaterialTheme.colorScheme.background)) {
            val sectioned = state.sort == SortOrder.DateDesc || state.sort == SortOrder.DateAsc
            val bottomSpace = PaddingValues(bottom = 110.dp)

            when {
                album != null -> MediaBrowser(
                    items = albumItems,
                    mode = state.viewMode,
                    columns = state.columns,
                    sectioned = sectioned,
                    onOpen = { viewer = albumItems to it },
                    onInfo = { infoFor = it },
                    modifier = Modifier.fillMaxSize(),
                    contentPadding = bottomSpace,
                    header = {
                        Column(Modifier.statusBarsPadding()) {
                            Row(verticalAlignment = Alignment.CenterVertically, modifier = Modifier.padding(top = 4.dp)) {
                                IconButton(onClick = { albumId = null }) {
                                    Icon(Icons.AutoMirrored.Filled.ArrowBack, "뒤로")
                                }
                                Text(album.name, style = MaterialTheme.typography.titleLarge, fontWeight = FontWeight.Bold,
                                    maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.weight(1f))
                            }
                            Toolbar(state, vm, count = albumItems.size)
                        }
                    },
                )

                tab == Tab.Photos -> MediaBrowser(
                    items = allSorted,
                    mode = state.viewMode,
                    columns = state.columns,
                    sectioned = sectioned,
                    onOpen = { viewer = allSorted to it },
                    onInfo = { infoFor = it },
                    modifier = Modifier.fillMaxSize(),
                    contentPadding = bottomSpace,
                    header = {
                        Column(Modifier.statusBarsPadding()) {
                            BigTitle("사진")
                            if (access == MediaAccess.Partial) PartialBanner(requestAgain)
                            Toolbar(state, vm, count = allSorted.size)
                        }
                    },
                )

                tab == Tab.Map -> MapScreen(
                    geo = state.geo,
                    scan = state.geoScan,
                    onOpen = { list, i -> viewer = list to i },
                    bottomPadding = bottomSpace,
                )

                else -> AlbumsPage(
                    albums = virtualAlbums(state.items) + state.albums,
                    onOpen = { albumId = it.id },
                    contentPadding = bottomSpace,
                )
            }

            if (state.loading) {
                CircularProgressIndicator(Modifier.align(Alignment.Center))
            } else if (state.items.isEmpty() && tab != Tab.Map) {
                Text("사진이 없습니다", Modifier.align(Alignment.Center), color = MaterialTheme.colorScheme.onSurfaceVariant)
            }

            if (viewer == null) {
                BottomBar(
                    tab = tab,
                    onTab = { tab = it; albumId = null },
                    modifier = Modifier.align(Alignment.BottomCenter),
                )
            }

            if (viewer == null) UpdateBanner(Modifier.align(Alignment.TopCenter))

            viewer?.let { (list, index) ->
                Viewer(items = list, startIndex = index, onClose = { viewer = null })
            }
        }

        infoFor?.let { item ->
            InfoSheet(
                item = item,
                onDismiss = { infoFor = null },
                showPreview = true,
                onOpen = {
                    val list = if (album != null) albumItems else allSorted
                    val i = list.indexOfFirst { it.uri == item.uri }
                    infoFor = null
                    if (i >= 0) viewer = list to i
                },
            )
        }
    }
}

private fun virtualAlbums(items: List<MediaItem>): List<Album> {
    if (items.isEmpty()) return emptyList()
    val newest = items.maxBy { it.date }
    val videos = items.filter { it.isVideo }
    return buildList {
        add(Album(ALL_ID, "최근 항목", newest, items.size))
        if (videos.isNotEmpty()) add(Album(VIDEOS_ID, "동영상", videos.maxBy { it.date }, videos.size))
    }
}

@Composable
private fun BigTitle(text: String) {
    Text(
        text,
        style = MaterialTheme.typography.headlineMedium,
        fontWeight = FontWeight.Bold,
        modifier = Modifier.padding(start = 20.dp, top = 28.dp, bottom = 12.dp),
    )
}

@Composable
private fun PartialBanner(requestAgain: () -> Unit) {
    Surface(
        shape = RoundedCornerShape(16.dp),
        color = MaterialTheme.colorScheme.surfaceContainerHigh,
        modifier = Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 4.dp),
    ) {
        Row(Modifier.padding(start = 16.dp, end = 4.dp), verticalAlignment = Alignment.CenterVertically) {
            Text("일부 사진만 볼 수 있게 허용됨", Modifier.weight(1f), style = MaterialTheme.typography.bodyMedium)
            TextButton(onClick = requestAgain) { Text("더 허용") }
        }
    }
}

/** 격자 ↔ 목록 전환, 정렬, 격자 칸 수. */
@Composable
private fun Toolbar(state: GalleryState, vm: GalleryViewModel, count: Int) {
    Row(
        Modifier.fillMaxWidth().padding(start = 16.dp, end = 8.dp, top = 4.dp, bottom = 4.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        SingleChoiceSegmentedButtonRow(Modifier.width(200.dp)) {
            val modes = listOf(ViewMode.Grid to "격자", ViewMode.List to "목록")
            modes.forEachIndexed { i, (mode, label) ->
                SegmentedButton(
                    selected = state.viewMode == mode,
                    onClick = { vm.setViewMode(mode) },
                    shape = SegmentedButtonDefaults.itemShape(i, modes.size),
                    icon = {
                        Icon(
                            if (mode == ViewMode.Grid) Icons.Outlined.GridView else Icons.AutoMirrored.Outlined.ViewList,
                            null, Modifier.size(18.dp),
                        )
                    },
                ) { Text(label) }
            }
        }
        Spacer(Modifier.weight(1f))
        Text("$count", style = MaterialTheme.typography.labelMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)

        var sortOpen by remember { mutableStateOf(false) }
        Box {
            IconButton(onClick = { sortOpen = true }) { Icon(Icons.AutoMirrored.Outlined.Sort, "정렬") }
            DropdownMenu(expanded = sortOpen, onDismissRequest = { sortOpen = false }) {
                SortOrder.entries.forEach { order ->
                    DropdownMenuItem(
                        text = {
                            Text(order.label, fontWeight = if (order == state.sort) FontWeight.Bold else FontWeight.Normal)
                        },
                        onClick = { vm.setSort(order); sortOpen = false },
                    )
                }
            }
        }

        if (state.viewMode == ViewMode.Grid) {
            IconButton(onClick = { vm.setColumns(state.columns + 1) }, enabled = state.columns < 7) {
                Icon(Icons.Outlined.Remove, "작게")
            }
            IconButton(onClick = { vm.setColumns(state.columns - 1) }, enabled = state.columns > 2) {
                Icon(Icons.Outlined.Add, "크게")
            }
        }
    }
}

@Composable
private fun AlbumsPage(albums: List<Album>, onOpen: (Album) -> Unit, contentPadding: PaddingValues) {
    val top = WindowInsets.statusBars.asPaddingValues().calculateTopPadding()
    LazyVerticalGrid(
        columns = GridCells.Fixed(3),
        modifier = Modifier.fillMaxSize(),
        contentPadding = PaddingValues(start = 12.dp, end = 12.dp, top = top, bottom = contentPadding.calculateBottomPadding()),
        horizontalArrangement = Arrangement.spacedBy(10.dp),
        verticalArrangement = Arrangement.spacedBy(14.dp),
    ) {
        item(span = { GridItemSpan(maxLineSpan) }) { BigTitle("앨범") }
        items(albums, key = { it.id }) { album ->
            Column(Modifier.clickable { onOpen(album) }) {
                AsyncImage(
                    model = album.cover.uri,
                    contentDescription = album.name,
                    contentScale = ContentScale.Crop,
                    modifier = Modifier
                        .fillMaxWidth()
                        .aspectRatio(1f)
                        .clip(RoundedCornerShape(18.dp))
                        .background(MaterialTheme.colorScheme.surfaceContainer),
                )
                Spacer(Modifier.height(6.dp))
                Text(album.name, style = MaterialTheme.typography.bodyMedium, fontWeight = FontWeight.Medium,
                    maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.padding(horizontal = 2.dp))
                Text("${album.count}", style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant, modifier = Modifier.padding(horizontal = 2.dp))
            }
        }
    }
}

/** 원 UI처럼 아래에 떠 있는 알약 모양 탭. */
@Composable
private fun BottomBar(tab: Tab, onTab: (Tab) -> Unit, modifier: Modifier = Modifier) {
    Surface(
        shape = RoundedCornerShape(36.dp),
        color = MaterialTheme.colorScheme.surfaceContainerHigh.copy(alpha = 0.94f),
        shadowElevation = 8.dp,
        modifier = modifier.navigationBarsPadding().padding(bottom = 12.dp),
    ) {
        Row(Modifier.padding(6.dp)) {
            TabButton(Icons.Outlined.Image, Icons.Filled.Image, "사진", tab == Tab.Photos) { onTab(Tab.Photos) }
            TabButton(Icons.Outlined.Collections, Icons.Filled.Collections, "앨범", tab == Tab.Albums) { onTab(Tab.Albums) }
            TabButton(Icons.Outlined.Map, Icons.Filled.Map, "지도", tab == Tab.Map) { onTab(Tab.Map) }
        }
    }
}

@Composable
private fun TabButton(icon: ImageVector, selectedIcon: ImageVector, label: String, selected: Boolean, onClick: () -> Unit) {
    val bg = if (selected) MaterialTheme.colorScheme.secondaryContainer else androidx.compose.ui.graphics.Color.Transparent
    Column(
        Modifier
            .clip(RoundedCornerShape(30.dp))
            .background(bg)
            .clickable(onClick = onClick)
            .padding(horizontal = 22.dp, vertical = 8.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        Icon(if (selected) selectedIcon else icon, null)
        Text(label, style = MaterialTheme.typography.labelMedium, fontWeight = if (selected) FontWeight.Bold else FontWeight.Normal)
    }
}
