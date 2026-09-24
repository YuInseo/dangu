package com.dangu.gallery.ui

import android.net.Uri
import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.lazy.LazyRow
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.outlined.OpenInNew
import androidx.compose.material.icons.outlined.Close
import androidx.compose.material.icons.outlined.PhotoLibrary
import androidx.compose.material.icons.outlined.Share
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import coil.compose.AsyncImage
import com.dangu.gallery.data.MediaItem
import com.dangu.gallery.data.MediaRepository

/**
 * 떠 있는 정보 패널. 뒤의 화면은 그대로 보이고, 바깥을 누르면 닫힌다.
 * [shared]가 비어 있으면 기기의 최근 사진을 보여 준다.
 */
@Composable
fun InfoPanelScreen(shared: List<Uri>, onClose: () -> Unit, onOpenGallery: () -> Unit) {
    Box(
        Modifier
            .fillMaxSize()
            .background(Color.Black.copy(alpha = 0.4f))
            .clickable(interactionSource = remember { MutableInteractionSource() }, indication = null, onClick = onClose),
    ) {
        BoxWithConstraints(
            Modifier
                .align(Alignment.BottomCenter)
                .statusBarsPadding()
                .navigationBarsPadding()
                .padding(10.dp),
        ) {
            Surface(
                shape = RoundedCornerShape(28.dp),
                color = MaterialTheme.colorScheme.surfaceContainer,
                shadowElevation = 12.dp,
                modifier = Modifier
                    .fillMaxWidth()
                    .heightIn(max = maxHeight * 0.88f)
                    // 카드 안을 누른 것이 바깥 눌림으로 새지 않게.
                    .clickable(interactionSource = remember { MutableInteractionSource() }, indication = null) {},
            ) {
                Column {
                    Row(
                        Modifier.fillMaxWidth().padding(start = 20.dp, end = 4.dp, top = 8.dp),
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        Text("사진 정보", style = MaterialTheme.typography.titleLarge, fontWeight = FontWeight.Bold,
                            modifier = Modifier.weight(1f))
                        IconButton(onClick = onOpenGallery) { Icon(Icons.Outlined.PhotoLibrary, "갤러리 열기") }
                        IconButton(onClick = onClose) { Icon(Icons.Outlined.Close, "닫기") }
                    }
                    // 내용이 길면 카드 높이 안에서 스크롤되도록 남은 높이만 준다.
                    Box(Modifier.weight(1f, fill = false)) {
                        if (shared.isNotEmpty()) {
                            SharedContent(shared)
                        } else {
                            WithMediaPermission { _, _ -> RecentContent() }
                        }
                    }
                }
            }
        }
    }
}

@Composable
private fun SharedContent(uris: List<Uri>) {
    val context = LocalContext.current
    val repo = remember(context) { MediaRepository.get(context) }
    var items by remember(uris) { mutableStateOf<List<MediaItem>?>(null) }
    LaunchedEffect(uris) { items = uris.map { repo.resolve(it) } }
    val list = items
    if (list == null) Loading() else Picker(list)
}

@Composable
private fun RecentContent() {
    val context = LocalContext.current
    val repo = remember(context) { MediaRepository.get(context) }
    var items by remember { mutableStateOf<List<MediaItem>?>(null) }
    LaunchedEffect(Unit) { items = repo.loadAll(limit = 40) }
    val list = items
    when {
        list == null -> Loading()
        list.isEmpty() -> Text("사진이 없습니다", Modifier.padding(24.dp))
        else -> Picker(list)
    }
}

@Composable
private fun Loading() {
    Box(Modifier.fillMaxWidth().height(200.dp), contentAlignment = Alignment.Center) {
        CircularProgressIndicator()
    }
}

/** 위에는 고를 수 있는 썸네일 줄, 아래에는 고른 사진과 그 정보. */
@Composable
private fun Picker(items: List<MediaItem>) {
    val context = LocalContext.current
    var selected by remember(items) { mutableIntStateOf(0) }
    val item = items[selected.coerceIn(0, items.lastIndex)]

    Column {
        if (items.size > 1) {
            LazyRow(
                contentPadding = PaddingValues(horizontal = 16.dp, vertical = 8.dp),
                horizontalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                itemsIndexed(items, key = { _, it -> it.uri.toString() }) { i, it ->
                    AsyncImage(
                        model = it.uri,
                        contentDescription = it.name,
                        contentScale = ContentScale.Crop,
                        modifier = Modifier
                            .size(64.dp)
                            .clip(RoundedCornerShape(12.dp))
                            .then(
                                if (i == selected) Modifier.border(
                                    BorderStroke(3.dp, MaterialTheme.colorScheme.primary), RoundedCornerShape(12.dp)
                                ) else Modifier
                            )
                            .clickable { selected = i },
                    )
                }
            }
        }

        Column(
            Modifier
                .weight(1f, fill = false)
                .verticalScroll(rememberScrollState())
                .padding(horizontal = 16.dp)
                .padding(bottom = 16.dp),
        ) {
            Box {
                AsyncImage(
                    model = item.uri,
                    contentDescription = item.name,
                    contentScale = ContentScale.Fit,
                    modifier = Modifier
                        .fillMaxWidth()
                        .height(220.dp)
                        .clip(RoundedCornerShape(18.dp))
                        .background(Color.Black)
                        .clickable { openExternally(context, item) },
                )
                Row(Modifier.align(Alignment.BottomEnd).padding(6.dp)) {
                    SmallRound(Icons.Outlined.Share, "공유") { share(context, item) }
                    Spacer(Modifier.size(6.dp))
                    SmallRound(Icons.AutoMirrored.Outlined.OpenInNew, "열기") { openExternally(context, item) }
                }
            }
            Spacer(Modifier.height(14.dp))
            InfoPanel(item)
        }
    }
}

@Composable
private fun SmallRound(icon: androidx.compose.ui.graphics.vector.ImageVector, label: String, onClick: () -> Unit) {
    Box(
        Modifier
            .size(40.dp)
            .clip(RoundedCornerShape(20.dp))
            .background(Color.Black.copy(alpha = 0.55f))
            .clickable(onClick = onClick),
        contentAlignment = Alignment.Center,
    ) {
        Icon(icon, label, tint = Color.White, modifier = Modifier.size(20.dp))
    }
}
