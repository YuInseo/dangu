package com.dangu.modes.ui

import android.content.Intent
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.detectDragGesturesAfterLongPress
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.systemBarsPadding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.grid.GridCells
import androidx.compose.foundation.lazy.grid.LazyVerticalGrid
import androidx.compose.foundation.lazy.grid.items
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.pager.HorizontalPager
import androidx.compose.foundation.pager.rememberPagerState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.Checkbox
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableStateMapOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.produceState
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.scale
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.graphics.Shadow
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.layout.boundsInRoot
import androidx.compose.ui.layout.onGloballyPositioned
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.ui.window.Dialog
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.dangu.modes.AppInfo
import com.dangu.modes.Apps
import com.dangu.modes.DItem
import com.dangu.modes.Desktop
import com.dangu.modes.Desktop.Companion.COLS
import com.dangu.modes.Desktop.Companion.DOCK
import com.dangu.modes.Desktop.Companion.DOCK_PAGE
import com.dangu.modes.Desktop.Companion.ROWS
import com.dangu.modes.IconCache
import com.dangu.modes.MainActivity
import com.dangu.modes.SecretDesktopActivity
import com.dangu.modes.Slot
import com.dangu.modes.Store
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import java.time.LocalTime
import java.time.format.DateTimeFormatter
import kotlin.math.roundToInt

private val labelStyle = TextStyle(
    color = Color.White, fontSize = 12.sp, textAlign = TextAlign.Center,
    shadow = Shadow(Color.Black.copy(alpha = 0.7f), Offset(0f, 1.5f), 4f),
)

/** 끌고 있는 항목과 손가락 위치(루트 좌표). */
private data class Drag(val from: Slot, val item: DItem, val pos: Offset)

/**
 * 새 바탕화면. 처음엔 비어 있고, 진짜 홈처럼 편집한다.
 *
 * - 빈 곳을 길게 누르거나 ⋮ → 바탕화면 편집으로 편집 모드.
 * - 편집 모드: 빈 칸을 누르면 앱 추가, 길게 눌러 끌면 옮기기, 앱 위에 놓으면 폴더, 폴더 위면 넣기,
 *   위의 🗑에 놓으면 삭제, 화면 가장자리에 머무르면 옆 페이지로. 페이지 추가·삭제, 아래 독.
 * - 앱을 길게 눌러 끌면 편집 모드로 바로 들어간다(보통 홈과 같이).
 */
@Composable
fun HomeScreen(activity: SecretDesktopActivity) {
    val store = remember { Store.get(activity) }
    val s by store.state.collectAsStateWithLifecycle()
    val desk = s.desktop
    var edit by activity.editMode
    val unlocked by activity.unlocked
    fun save(d: Desktop) = store.update { copy(desktop = d) }

    if (!unlocked) {
        LockedView { activity.authenticate() }
        return
    }

    val pager = rememberPagerState { desk.pages.size }
    val scope = rememberCoroutineScope()
    var drag by remember { mutableStateOf<Drag?>(null) }
    val itemBounds = remember { mutableStateMapOf<Slot, Rect>() }
    val gridBounds = remember { mutableStateMapOf<Int, Rect>() }
    var dockBounds by remember { mutableStateOf(Rect.Zero) }
    var trashBounds by remember { mutableStateOf(Rect.Zero) }
    var rootWidth by remember { mutableStateOf(0f) }
    var addTo by remember { mutableStateOf<Slot?>(null) }
    var addMany by remember { mutableStateOf(false) }
    var openFolder by remember { mutableStateOf<Slot?>(null) }
    var menu by remember { mutableStateOf(false) }
    val edgePx = with(LocalDensity.current) { 28.dp.toPx() }
    val deskNow by rememberUpdatedState(desk)

    /** 손가락이 가리키는 곳: 삭제 칸이면 null과 true, 아니면 칸. */
    fun slotAt(p: Offset): Slot? {
        if (dockBounds.contains(p)) {
            val cell = ((p.x - dockBounds.left) / (dockBounds.width / DOCK)).toInt().coerceIn(0, DOCK - 1)
            return Slot(DOCK_PAGE, cell)
        }
        val g = gridBounds[pager.currentPage] ?: return null
        if (!g.contains(p)) return null
        val col = ((p.x - g.left) / (g.width / COLS)).toInt().coerceIn(0, COLS - 1)
        val row = ((p.y - g.top) / (g.height / ROWS)).toInt().coerceIn(0, ROWS - 1)
        return Slot(pager.currentPage, row * COLS + col)
    }

    // 끄는 중 가장자리에 머물면 옆 페이지로(빈 페이지를 만들어서라도).
    val edge = drag?.pos?.x?.let { x -> if (x < edgePx) -1 else if (rootWidth > 0 && x > rootWidth - edgePx) 1 else 0 } ?: 0
    LaunchedEffect(edge) {
        if (edge == 0) return@LaunchedEffect
        delay(650)
        val next = pager.currentPage + edge
        if (next >= 0) {
            if (next >= deskNow.pages.size) save(deskNow.addPage())
            delay(50)
            pager.animateScrollToPage(next)
        }
    }

    Box(
        Modifier
            .fillMaxSize()
            .background(Color.Black.copy(alpha = if (edit) 0.45f else 0.2f))
            .onGloballyPositioned { rootWidth = it.size.width.toFloat() }
            .systemBarsPadding()
    ) {
        Column(Modifier.fillMaxSize()) {
            // ── 윗줄: 평소엔 시계와 ⋮, 편집 중엔 편집 도구, 끄는 중엔 삭제 칸 ──
            Box(Modifier.fillMaxWidth().height(96.dp)) {
                when {
                    drag != null -> {
                        val over = drag?.pos?.let { trashBounds.contains(it) } == true
                        Box(
                            Modifier
                                .align(Alignment.Center)
                                .padding(horizontal = 24.dp)
                                .fillMaxWidth()
                                .height(56.dp)
                                .clip(RoundedCornerShape(28.dp))
                                .background(if (over) Color(0xCCE5484D) else Color(0x55FFFFFF))
                                .onGloballyPositioned { trashBounds = it.boundsInRoot() },
                            contentAlignment = Alignment.Center,
                        ) { Text("🗑  여기에 놓으면 삭제", color = Color.White, fontWeight = FontWeight.SemiBold) }
                    }
                    edit -> Row(
                        Modifier.align(Alignment.Center).padding(horizontal = 12.dp),
                        horizontalArrangement = Arrangement.spacedBy(6.dp),
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        Pill("+ 앱") { addMany = true }
                        Pill("+ 페이지") {
                            save(desk.addPage())
                            scope.launch { delay(60); pager.animateScrollToPage(desk.pages.size) }
                        }
                        if (desk.pages.size > 1 && desk.pages.getOrNull(pager.currentPage).isNullOrEmpty()) {
                            Pill("페이지 삭제") { save(desk.removePage(pager.currentPage)) }
                        }
                        Pill("완료", strong = true) { edit = false }
                    }
                    else -> {
                        Clock(Modifier.align(Alignment.Center))
                        Box(Modifier.align(Alignment.TopEnd)) {
                            Text(
                                "⋮", color = Color.White, fontSize = 26.sp,
                                modifier = Modifier
                                    .semantics { contentDescription = "바탕화면 메뉴" }
                                    .clickable { menu = true }
                                    .padding(horizontal = 18.dp, vertical = 8.dp),
                            )
                            DropdownMenu(expanded = menu, onDismissRequest = { menu = false }) {
                                DropdownMenuItem(text = { Text("바탕화면 편집") }, onClick = { menu = false; edit = true })
                                DropdownMenuItem(text = { Text("기본 바탕화면으로") }, onClick = {
                                    menu = false
                                    activity.startActivity(Intent(Intent.ACTION_MAIN).addCategory(Intent.CATEGORY_HOME).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
                                })
                                DropdownMenuItem(text = { Text("모드 스위치 설정") }, onClick = {
                                    menu = false
                                    activity.startActivity(Intent(activity, MainActivity::class.java))
                                })
                            }
                        }
                    }
                }
            }

            // ── 페이지들 ──
            HorizontalPager(
                state = pager,
                modifier = Modifier.weight(1f),
                beyondViewportPageCount = desk.pages.size,
                userScrollEnabled = drag == null,
                key = { it },
            ) { page ->
                val cells = desk.pages.getOrNull(page) ?: emptyMap()
                Column(
                    Modifier
                        .fillMaxSize()
                        .padding(horizontal = 12.dp)
                        .onGloballyPositioned { gridBounds[page] = it.boundsInRoot() }
                        .pointerInput(edit) {
                            // 빈 곳 길게 → 편집 모드
                            detectTapGestures(onLongPress = { if (!edit) edit = true })
                        }
                ) {
                    for (row in 0 until ROWS) {
                        Row(Modifier.weight(1f).fillMaxWidth()) {
                            for (col in 0 until COLS) {
                                val slot = Slot(page, row * COLS + col)
                                Box(Modifier.weight(1f).fillMaxHeight(), contentAlignment = Alignment.Center) {
                                    Cell(
                                        slot = slot,
                                        item = cells[slot.cell],
                                        edit = edit,
                                        hidden = drag?.from == slot,
                                        hovered = drag != null && drag?.from != slot && drag?.pos?.let { slotAt(it) } == slot,
                                        onBounds = { itemBounds[slot] = it },
                                        onOpen = { item -> open(activity, item) { openFolder = slot } },
                                        onAdd = { addTo = slot },
                                        onDragStart = { item, local ->
                                            edit = true
                                            val b = itemBounds[slot] ?: Rect.Zero
                                            drag = Drag(slot, item, b.topLeft + local)
                                        },
                                        onDrag = { d -> drag = drag?.let { it.copy(pos = it.pos + d) } },
                                        onDragEnd = {
                                            drag?.let { dr -> finishDrag(dr, trashBounds, ::slotAt, deskNow, ::save) }
                                            drag = null
                                        },
                                    )
                                }
                            }
                        }
                    }
                }
            }

            // 페이지 점
            Row(Modifier.fillMaxWidth().padding(vertical = 8.dp), horizontalArrangement = Arrangement.Center) {
                repeat(desk.pages.size) { i ->
                    Box(
                        Modifier.padding(horizontal = 3.dp).size(if (i == pager.currentPage) 8.dp else 6.dp).clip(CircleShape)
                            .background(Color.White.copy(alpha = if (i == pager.currentPage) 0.95f else 0.45f))
                    )
                }
            }

            // ── 독 ──
            Row(
                Modifier
                    .fillMaxWidth()
                    .padding(horizontal = 12.dp, vertical = 8.dp)
                    .clip(RoundedCornerShape(28.dp))
                    .background(Color.White.copy(alpha = 0.14f))
                    .onGloballyPositioned { dockBounds = it.boundsInRoot() }
                    .padding(vertical = 10.dp),
            ) {
                for (i in 0 until DOCK) {
                    val slot = Slot(DOCK_PAGE, i)
                    Box(Modifier.weight(1f).height(84.dp), contentAlignment = Alignment.Center) {
                        Cell(
                            slot = slot, item = desk.dock[i], edit = edit, showLabel = false,
                            hidden = drag?.from == slot,
                            hovered = drag != null && drag?.from != slot && drag?.pos?.let { slotAt(it) } == slot,
                            onBounds = { itemBounds[slot] = it },
                            onOpen = { item -> open(activity, item) { openFolder = slot } },
                            onAdd = { addTo = slot },
                            onDragStart = { item, local ->
                                edit = true
                                drag = Drag(slot, item, (itemBounds[slot] ?: Rect.Zero).topLeft + local)
                            },
                            onDrag = { d -> drag = drag?.let { it.copy(pos = it.pos + d) } },
                            onDragEnd = {
                                drag?.let { dr -> finishDrag(dr, trashBounds, ::slotAt, deskNow, ::save) }
                                drag = null
                            },
                        )
                    }
                }
            }
        }

        if (desk.isEmpty && !edit) {
            Column(Modifier.align(Alignment.Center).padding(32.dp), horizontalAlignment = Alignment.CenterHorizontally) {
                Text("텅 빈 새 바탕화면", style = labelStyle.copy(fontSize = 18.sp, fontWeight = FontWeight.SemiBold))
                Spacer(Modifier.height(6.dp))
                Text("빈 곳을 길게 누르면 바탕화면 편집", style = labelStyle.copy(fontSize = 14.sp))
            }
        }

        // 손가락을 따라다니는 항목
        drag?.let { d ->
            val half = with(LocalDensity.current) { 32.dp.toPx() }
            Box(Modifier.offset { IntOffset((d.pos.x - half).roundToInt(), (d.pos.y - half - 40).roundToInt()) }) {
                Box(Modifier.scale(1.15f)) { ItemIcon(d.item, 64) }
            }
        }
    }

    // ── 대화 상자들 ──
    addTo?.let { slot ->
        AppPicker(activity, multi = false, exclude = desk.allApps(), onDone = { pkgs ->
            pkgs.firstOrNull()?.let { save(desk.put(slot, DItem.App(it))) }
            addTo = null
        })
    }
    if (addMany) {
        AppPicker(activity, multi = true, exclude = desk.allApps(), onDone = { pkgs ->
            if (pkgs.isNotEmpty()) save(desk.addApps(pkgs, pager.currentPage))
            addMany = false
        })
    }
    openFolder?.let { slot ->
        (desk.at(slot) as? DItem.Folder)?.let { f ->
            FolderDialog(
                folder = f, edit = edit,
                onLaunch = { pkg -> launch(activity, pkg) },
                onRename = { save(desk.renameFolder(slot, it)) },
                onTakeOut = { pkg -> save(desk.takeOutOfFolder(slot, pkg)) },
                onDismiss = { openFolder = null },
            )
        } ?: run { openFolder = null }
    }
}

private fun finishDrag(d: Drag, trash: Rect, slotAt: (Offset) -> Slot?, desk: Desktop, save: (Desktop) -> Unit) {
    when {
        trash.contains(d.pos) -> save(desk.remove(d.from))
        else -> slotAt(d.pos)?.let { to -> save(desk.drop(d.from, to)) }
    }
}

private fun open(activity: SecretDesktopActivity, item: DItem, openFolder: () -> Unit) {
    when (item) {
        is DItem.App -> launch(activity, item.pkg)
        is DItem.Folder -> openFolder()
    }
}

private fun launch(activity: SecretDesktopActivity, pkg: String) {
    activity.packageManager.getLaunchIntentForPackage(pkg)?.let {
        runCatching { activity.startActivity(it.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)) }
    }
}

@Composable
private fun Cell(
    slot: Slot,
    item: DItem?,
    edit: Boolean,
    hidden: Boolean,
    hovered: Boolean,
    onBounds: (Rect) -> Unit,
    onOpen: (DItem) -> Unit,
    onAdd: () -> Unit,
    onDragStart: (DItem, Offset) -> Unit,
    onDrag: (Offset) -> Unit,
    onDragEnd: () -> Unit,
    showLabel: Boolean = true,
) {
    val context = LocalContext.current
    val ring = if (hovered) Modifier.border(2.dp, Color.White, RoundedCornerShape(18.dp)) else Modifier
    if (item == null) {
        if (edit) {
            Box(
                Modifier
                    .size(64.dp)
                    .then(ring)
                    .clip(RoundedCornerShape(18.dp))
                    .border(1.dp, Color.White.copy(alpha = 0.35f), RoundedCornerShape(18.dp))
                    .semantics { contentDescription = "빈 칸 ${slot.page}:${slot.cell}" }
                    .clickable(onClick = onAdd),
                contentAlignment = Alignment.Center,
            ) { Text("+", color = Color.White.copy(alpha = 0.7f), fontSize = 22.sp) }
        } else if (hovered) {
            Box(Modifier.size(64.dp).then(ring))
        }
        return
    }
    val wiggle by animateFloatAsState(if (edit) 0.94f else 1f, label = "edit-scale")
    val start by rememberUpdatedState(onDragStart)
    val move by rememberUpdatedState(onDrag)
    val end by rememberUpdatedState(onDragEnd)
    val current by rememberUpdatedState(item)
    Column(
        Modifier
            .onGloballyPositioned { onBounds(it.boundsInRoot()) }
            .then(ring)
            .scale(wiggle)
            .semantics { contentDescription = (if (item is DItem.App) IconCache.label(context, item.pkg) else (item as DItem.Folder).name) }
            .pointerInput(slot) {
                detectDragGesturesAfterLongPress(
                    onDragStart = { start(current, it) },
                    onDragEnd = { end() },
                    onDragCancel = { end() },
                ) { change, amount -> change.consume(); move(amount) }
            }
            .pointerInput(slot, edit) { detectTapGestures(onTap = { onOpen(current) }) }
            .then(if (hidden) Modifier.scale(0f) else Modifier),
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        ItemIcon(item, 54)
        if (showLabel) {
            Spacer(Modifier.height(4.dp))
            Text(
                if (item is DItem.App) IconCache.label(context, item.pkg) else (item as DItem.Folder).name,
                style = labelStyle, maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.width(76.dp),
            )
        }
    }
}

@Composable
private fun ItemIcon(item: DItem, sizeDp: Int) {
    when (item) {
        is DItem.App -> AppIcon(item.pkg, sizeDp)
        is DItem.Folder -> Box(
            Modifier.size(sizeDp.dp).clip(RoundedCornerShape((sizeDp * 0.3f).dp)).background(Color.White.copy(alpha = 0.85f)).padding((sizeDp * 0.12f).dp),
        ) {
            Column(verticalArrangement = Arrangement.SpaceBetween, modifier = Modifier.fillMaxSize()) {
                item.apps.take(4).chunked(2).forEach { pair ->
                    Row(horizontalArrangement = Arrangement.SpaceBetween, modifier = Modifier.fillMaxWidth()) {
                        pair.forEach { AppIcon(it, (sizeDp * 0.34f).roundToInt()) }
                    }
                }
            }
        }
    }
}

@Composable
private fun AppIcon(pkg: String, sizeDp: Int) {
    val context = LocalContext.current
    val icon by produceState<ImageBitmap?>(IconCache.cached(pkg), pkg) { value = IconCache.icon(context, pkg) }
    val i = icon
    if (i != null) Image(i, null, Modifier.size(sizeDp.dp))
    else Box(Modifier.size(sizeDp.dp).clip(RoundedCornerShape((sizeDp * 0.3f).dp)).background(Accent))
}

@Composable
private fun Pill(text: String, strong: Boolean = false, onClick: () -> Unit) {
    Box(
        Modifier
            .clip(RoundedCornerShape(20.dp))
            .background(if (strong) Accent else Color.White.copy(alpha = 0.22f))
            .clickable(onClick = onClick)
            .padding(horizontal = 14.dp, vertical = 9.dp),
    ) { Text(text, color = Color.White, fontSize = 14.sp, fontWeight = FontWeight.SemiBold) }
}

@Composable
private fun Clock(modifier: Modifier) {
    var now by remember { mutableStateOf(LocalTime.now()) }
    LaunchedEffect(Unit) { while (true) { now = LocalTime.now(); delay(10_000) } }
    Text(now.format(DateTimeFormatter.ofPattern("HH:mm")), style = labelStyle.copy(fontSize = 48.sp, fontWeight = FontWeight.Light), modifier = modifier)
}

@Composable
private fun LockedView(onUnlock: () -> Unit) {
    Column(
        Modifier.fillMaxSize().background(Color.Black.copy(alpha = 0.6f)),
        verticalArrangement = Arrangement.Center,
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        Text("🔒", fontSize = 56.sp)
        Spacer(Modifier.height(12.dp))
        Text("비밀 바탕화면이 잠겨 있어요", color = Color.White)
        Spacer(Modifier.height(16.dp))
        Button(onClick = onUnlock) { Text("잠금 해제") }
    }
}

@Composable
private fun FolderDialog(
    folder: DItem.Folder,
    edit: Boolean,
    onLaunch: (String) -> Unit,
    onRename: (String) -> Unit,
    onTakeOut: (String) -> Unit,
    onDismiss: () -> Unit,
) {
    val context = LocalContext.current
    var name by remember(folder.id) { mutableStateOf(folder.name) }
    Dialog(onDismissRequest = { if (edit) onRename(name); onDismiss() }) {
        Column(
            Modifier.clip(RoundedCornerShape(28.dp)).background(Color(0xF2222834)).padding(20.dp),
            horizontalAlignment = Alignment.CenterHorizontally,
        ) {
            if (edit) {
                OutlinedTextField(name, { name = it }, singleLine = true, label = { Text("폴더 이름") })
            } else {
                Text(folder.name, color = Color.White, fontSize = 20.sp, fontWeight = FontWeight.SemiBold)
            }
            Spacer(Modifier.height(16.dp))
            LazyVerticalGrid(GridCells.Fixed(3), Modifier.heightIn(max = 360.dp), verticalArrangement = Arrangement.spacedBy(14.dp)) {
                items(folder.apps, key = { it }) { pkg ->
                    Column(
                        Modifier.clickable { if (edit) onTakeOut(pkg) else { onLaunch(pkg); onDismiss() } },
                        horizontalAlignment = Alignment.CenterHorizontally,
                    ) {
                        Box {
                            AppIcon(pkg, 52)
                            if (edit) Text(
                                "−", color = Color.White, fontSize = 14.sp, fontWeight = FontWeight.Bold,
                                modifier = Modifier.align(Alignment.TopEnd).size(20.dp).clip(CircleShape).background(Color(0xFFE5484D)),
                                textAlign = TextAlign.Center,
                            )
                        }
                        Spacer(Modifier.height(4.dp))
                        Text(IconCache.label(context, pkg), color = Color.White, fontSize = 12.sp, maxLines = 1, overflow = TextOverflow.Ellipsis)
                    }
                }
            }
            if (edit) {
                Spacer(Modifier.height(8.dp))
                Text("앱을 누르면 폴더에서 빼서 바탕화면에 둡니다", color = Color.White.copy(alpha = 0.7f), fontSize = 12.sp)
            }
        }
    }
}

/** 앱 고르기. multi면 여러 개를 골라 한꺼번에, 아니면 하나 누르면 끝. */
@Composable
private fun AppPicker(activity: SecretDesktopActivity, multi: Boolean, exclude: Set<String>, onDone: (List<String>) -> Unit) {
    val apps by produceState<List<AppInfo>?>(null) { value = Apps.launchable(activity) }
    var query by remember { mutableStateOf("") }
    var chosen by remember { mutableStateOf(listOf<String>()) }
    AlertDialog(
        onDismissRequest = { onDone(emptyList()) },
        title = { Text(if (multi) "바탕화면에 앱 추가" else "이 칸에 둘 앱") },
        text = {
            Column {
                OutlinedTextField(query, { query = it }, placeholder = { Text("검색") }, singleLine = true)
                Spacer(Modifier.height(8.dp))
                val list = apps?.filter { it.packageName !in exclude && (query.isBlank() || it.label.contains(query, true)) }
                if (list == null) Text("불러오는 중…")
                else LazyColumn(Modifier.heightIn(max = 380.dp)) {
                    items(list, key = { it.packageName }) { a ->
                        key(a.packageName) {
                            Row(
                                Modifier.fillMaxWidth().clickable {
                                    if (multi) chosen = if (a.packageName in chosen) chosen - a.packageName else chosen + a.packageName
                                    else onDone(listOf(a.packageName))
                                }.padding(vertical = 6.dp),
                                verticalAlignment = Alignment.CenterVertically,
                            ) {
                                if (multi) Checkbox(a.packageName in chosen, null)
                                if (a.icon != null) Image(a.icon.asImageBitmap(), null, Modifier.size(32.dp))
                                Spacer(Modifier.width(12.dp))
                                Text(a.label, maxLines = 1, overflow = TextOverflow.Ellipsis)
                            }
                        }
                    }
                }
            }
        },
        confirmButton = {
            if (multi) TextButton(onClick = { onDone(chosen) }) { Text("추가 ${chosen.size}개") }
            else TextButton(onClick = { onDone(emptyList()) }) { Text("닫기") }
        },
        dismissButton = { if (multi) TextButton(onClick = { onDone(emptyList()) }) { Text("취소") } },
    )
}
