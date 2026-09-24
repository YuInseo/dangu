@file:OptIn(ExperimentalMaterial3Api::class, ExperimentalLayoutApi::class)

package com.dangu.lumen.ui

import android.Manifest
import android.os.Build
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.detectDragGestures
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.layout.systemBarsPadding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.Star
import androidx.compose.material.icons.outlined.Home
import androidx.compose.material.icons.outlined.Refresh
import androidx.compose.material.icons.outlined.Info
import androidx.compose.material.icons.outlined.Menu
import androidx.compose.material3.Button
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Slider
import androidx.compose.material3.Surface
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.material3.rememberModalBottomSheetState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.dp
import androidx.compose.ui.viewinterop.AndroidView
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.dangu.lumen.BuildConfig
import com.dangu.lumen.LumenTheme
import com.dangu.lumen.MainActivity
import com.dangu.lumen.Prefs
import com.dangu.lumen.Themes
import com.dangu.lumen.Updater
import kotlinx.coroutines.launch
import kotlin.math.roundToInt

@Composable
fun LumenScreen(activity: MainActivity) {
    val prefs by activity.prefs.state.collectAsStateWithLifecycle()
    val theme = Themes.byId(prefs.themeId)
    val accent = if (theme.id == "original") Color(0xFF5865F2) else Color(theme.accent)
    val scheme = if (theme.light) lightColorScheme(primary = accent) else darkColorScheme(
        primary = accent,
        surface = Color(if (theme.id == "original") 0xFF2B2D31 else theme.side),
        surfaceContainerLow = Color(if (theme.id == "original") 0xFF2B2D31 else theme.side),
        surfaceContainerHigh = Color(if (theme.id == "original") 0xFF383A40 else theme.surface),
    )
    val bg = Color(if (theme.id == "original") 0xFF1E1F22 else theme.base)

    MaterialTheme(colorScheme = scheme) {
        var showSettings by remember { mutableStateOf(false) }
        var drawerOpen by remember { mutableStateOf(false) }

        val discord by activity.discord
        val chat by activity.chat
        val showWeb by activity.showWeb
        val fullscreen = activity.fullscreen.value
        val loggedIn = discord.ready && discord.me != null
        // 로그인하고 나면 화면 전체가 옛날 디스코드 앱(네이티브). 웹 화면은 로그인·음성·설정 때만.
        val native = prefs.classic && loggedIn && !showWeb && fullscreen == null

        // WebView는 이 층 아래(평범한 뷰)에 있다. 네이티브 화면이면 그 위를 덮는다.
        BoxWithConstraints(Modifier.fillMaxSize()) {
            if (native) {
                Box(
                    Modifier
                        .fillMaxSize()
                        // 아래 WebView로 터치가 새지 않게
                        .pointerInput(Unit) { awaitPointerEventScope { while (true) awaitPointerEvent() } }
                ) {
                    NativeChat(
                        activity = activity,
                        discord = discord,
                        chat = chat,
                        onMenu = { drawerOpen = true },
                        onSettings = { showSettings = true },
                    )
                }
            } else {
                PageOverlay(activity, prefs)

                Bubble(
                    prefs = prefs,
                    maxW = constraints.maxWidth.toFloat(),
                    maxH = constraints.maxHeight.toFloat(),
                    onMove = { x, y -> activity.prefs.update { copy(bubbleX = x, bubbleY = y) } },
                    // 웹 화면을 보고 있으면 누르면 앱 화면으로, 길게 누르면 설정.
                    onTap = {
                        if (prefs.classic && loggedIn) activity.showWeb.value = false else showSettings = true
                    },
                    onLongPress = {
                        if (prefs.classic) showSettings = true
                        else activity.prefs.update { copy(hideSidebar = !hideSidebar) }
                    },
                )
            }

            UpdateBanner(Modifier.align(Alignment.TopCenter))

            if (native) {
                ClassicDrawer(
                    activity = activity,
                    open = drawerOpen,
                    onOpenChange = { drawerOpen = it },
                    onSettings = { drawerOpen = false; showSettings = true },
                )
            }
        }

        if (showSettings) {
            SettingsSheet(activity, prefs, onDismiss = { showSettings = false })
        }
    }
}

/**
 * 화면 가장자리에 떠 있는 작은 단추. 끌어서 옮기고, 누르면 설정, 길게 누르면 사이드바 숨기기/보이기.
 * 위치는 화면 비율로 기억한다(회전해도 같은 자리).
 */
@Composable
private fun Bubble(
    prefs: Prefs.Snapshot,
    maxW: Float,
    maxH: Float,
    onMove: (Float, Float) -> Unit,
    onTap: () -> Unit,
    onLongPress: () -> Unit,
) {
    val size = with(LocalDensity.current) { 44.dp.toPx() }
    val startX = if (prefs.bubbleX < 0) 1f else prefs.bubbleX
    var fx by remember { mutableFloatStateOf(startX) }
    var fy by remember { mutableFloatStateOf(prefs.bubbleY) }
    var dragging by remember { mutableStateOf(false) }
    val rangeX = (maxW - size).coerceAtLeast(1f)
    val rangeY = (maxH - size).coerceAtLeast(1f)

    Box(
        Modifier
            .offset { IntOffset((fx * rangeX).roundToInt(), (fy * rangeY).roundToInt()) }
            .size(44.dp)
            .alpha(if (dragging) 0.95f else 0.6f)
            .clip(CircleShape)
            .background(MaterialTheme.colorScheme.primary)
            .border(2.dp, Color.White.copy(alpha = 0.35f), CircleShape)
            .pointerInput(rangeX, rangeY) {
                detectDragGestures(
                    onDragStart = { dragging = true },
                    onDragEnd = {
                        dragging = false
                        // 가까운 옆 가장자리에 붙인다
                        fx = if (fx < 0.5f) 0f else 1f
                        onMove(fx, fy)
                    },
                    onDragCancel = { dragging = false },
                ) { change, amount ->
                    change.consume()
                    fx = (fx + amount.x / rangeX).coerceIn(0f, 1f)
                    fy = (fy + amount.y / rangeY).coerceIn(0f, 1f)
                }
            }
            .pointerInput(Unit) {
                detectTapGestures(onTap = { onTap() }, onLongPress = { onLongPress() })
            },
        contentAlignment = Alignment.Center,
    ) {
        Icon(Icons.Outlined.Star, "Lumen 설정", tint = Color.White, modifier = Modifier.size(22.dp))
    }
}

@Composable
private fun SettingsSheet(activity: MainActivity, prefs: Prefs.Snapshot, onDismiss: () -> Unit) {
    val sheet = rememberModalBottomSheetState(skipPartiallyExpanded = true)
    val scope = rememberCoroutineScope()
    ModalBottomSheet(onDismissRequest = onDismiss, sheetState = sheet) {
        Column(
            Modifier
                .fillMaxWidth()
                .verticalScroll(rememberScrollState())
                .padding(horizontal = 20.dp)
                .padding(bottom = 32.dp),
            verticalArrangement = Arrangement.spacedBy(14.dp),
        ) {
            Text("Lumen", style = MaterialTheme.typography.headlineSmall, fontWeight = FontWeight.Bold)

            // 좁은 화면에서 단추가 잘리지 않게 줄을 넘긴다.
            FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                OutlinedButton(onClick = { activity.prefs.update { copy(hideSidebar = !hideSidebar) } }) {
                    Icon(Icons.Outlined.Menu, null, Modifier.size(18.dp))
                    Spacer(Modifier.width(6.dp))
                    Text(if (prefs.hideSidebar) "사이드바 보이기" else "사이드바 숨기기")
                }
                OutlinedButton(onClick = { activity.reload(); onDismiss() }) {
                    Icon(Icons.Outlined.Refresh, null, Modifier.size(18.dp))
                }
                OutlinedButton(onClick = { activity.goHome(); onDismiss() }) {
                    Icon(Icons.Outlined.Home, null, Modifier.size(18.dp))
                }
            }
            Toggle(
                "옛날 디스코드 서랍",
                "왼쪽 가장자리에서 밀거나 단추를 누르면 서버 막대와 채널 목록이 나옵니다. 설정은 단추를 길게.",
                prefs.classic,
            ) { on ->
                activity.prefs.update { copy(classic = on) }
                activity.reload()
            }
            Text(
                if (prefs.classic) "클래식 서랍을 끄면 단추 길게 누르기가 사이드바 숨기기로 바뀝니다."
                else "단추를 길게 누르면 사이드바(서버·채널 목록)를 바로 숨기거나 보입니다.",
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )

            HorizontalDivider()
            Text("테마", style = MaterialTheme.typography.titleMedium, fontWeight = FontWeight.SemiBold)
            FlowRow(horizontalArrangement = Arrangement.spacedBy(10.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
                Themes.all.forEach { t ->
                    ThemeChip(t, selected = t.id == prefs.themeId) { activity.prefs.update { copy(themeId = t.id) } }
                }
            }

            HorizontalDivider()
            Text("화면", style = MaterialTheme.typography.titleMedium, fontWeight = FontWeight.SemiBold)
            Toggle("멤버 목록 숨기기", "채팅 오른쪽의 멤버 목록", prefs.hideMembers) {
                activity.prefs.update { copy(hideMembers = it) }
            }
            Toggle("Nitro·선물 권유 숨기기", null, prefs.hideNitro) {
                activity.prefs.update { copy(hideNitro = it) }
            }
            Toggle("넓은 레이아웃", "PC처럼 넓게 그리고 줄여서 보여 줍니다", prefs.wideLayout) {
                activity.prefs.update { copy(wideLayout = it) }
            }
            LabeledSlider("글자 크기 ${prefs.textZoom}%", prefs.textZoom.toFloat(), 70f..140f, 13) {
                activity.prefs.update { copy(textZoom = it.roundToInt()) }
            }
            LabeledSlider("모서리 둥글기 ${prefs.radius}px", prefs.radius.toFloat(), 0f..24f, 23) {
                activity.prefs.update { copy(radius = it.roundToInt()) }
            }

            HorizontalDivider()
            Toggle("알림", "앱이 뒤에 있을 때 새 메시지·멘션 알림", prefs.notifications) { on ->
                activity.prefs.update { copy(notifications = on) }
                if (on && Build.VERSION.SDK_INT >= 33) activity.notificationPermission.launch(Manifest.permission.POST_NOTIFICATIONS)
                if (on) activity.reload()
            }

            HorizontalDivider()
            Text("문제 해결", style = MaterialTheme.typography.titleMedium, fontWeight = FontWeight.SemiBold)
            Toggle("안전 모드", "테마·스크립트를 전부 끄고 디스코드 그대로", prefs.safeMode) {
                activity.retry(safe = it)
            }
            Row(verticalAlignment = Alignment.CenterVertically) {
                Column(Modifier.weight(1f)) {
                    Text("브라우저 종류", style = MaterialTheme.typography.bodyLarge)
                    Text(UA_NAMES[prefs.uaMode], style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                }
                TextButton(onClick = { activity.retry(nextUa = true) }) { Text("바꾸기") }
            }

            HorizontalDivider()
            Text("사용자 CSS", style = MaterialTheme.typography.titleMedium, fontWeight = FontWeight.SemiBold)
            var css by remember { mutableStateOf(prefs.customCss) }
            OutlinedTextField(
                value = css,
                onValueChange = { css = it },
                modifier = Modifier.fillMaxWidth().heightIn(min = 120.dp),
                textStyle = MaterialTheme.typography.bodySmall.copy(fontFamily = FontFamily.Monospace),
                placeholder = { Text("/* 예: 메시지 글꼴 바꾸기 */\n[class*=\"messageContent_\"] { font-size: 15px; }") },
            )
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                Button(onClick = { activity.prefs.update { copy(customCss = css) } }) { Text("적용") }
                TextButton(onClick = { css = ""; activity.prefs.update { copy(customCss = "") } }) { Text("지우기") }
            }

            HorizontalDivider()
            val updater = remember { Updater.get(activity) }
            val ustate by updater.state.collectAsStateWithLifecycle()
            Row(verticalAlignment = Alignment.CenterVertically) {
                Column(Modifier.weight(1f)) {
                    Text("버전 ${BuildConfig.VERSION_NAME}", style = MaterialTheme.typography.bodyMedium)
                    Text(
                        when (val u = ustate) {
                            Updater.State.Checking -> "확인 중…"
                            Updater.State.UpToDate -> "최신 버전입니다"
                            is Updater.State.Downloading -> "새 버전 ${u.versionName} 받는 중"
                            is Updater.State.Ready -> "새 버전 ${u.versionName} 준비됨"
                            is Updater.State.Failed -> "확인 실패: ${u.message}"
                            else -> ""
                        },
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                }
                if (ustate is Updater.State.Ready) {
                    Button(onClick = { updater.install(activity) }) { Text("설치") }
                } else {
                    TextButton(onClick = { scope.launch { updater.check() } }) { Text("업데이트 확인") }
                }
            }
            Text(
                "Lumen은 디스코드 공식 웹 클라이언트를 그대로 띄우고 모양만 바꿉니다. 로그인은 디스코드 페이지에서 " +
                    "하며 앱은 비밀번호나 토큰을 따로 보관하지 않습니다. 디스코드와 관계없는 비공식 앱입니다.",
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
    }
}

private val UA_NAMES = listOf("데스크톱 크롬", "WebView 기본", "모바일 크롬")

/**
 * 위에는 읽는 중 막대, 안 뜨면 가운데에 이유와 해결 단추.
 * 검은 화면만 남아 아무것도 알 수 없는 일이 없게 한다.
 */
@Composable
private fun PageOverlay(activity: MainActivity, prefs: Prefs.Snapshot) {
    val page by activity.page
    Box(Modifier.fillMaxSize().systemBarsPadding()) {
        if (page.progress in 1..99) {
            LinearProgressIndicator(
                progress = { page.progress / 100f },
                modifier = Modifier.fillMaxWidth().height(3.dp).align(Alignment.TopCenter),
            )
        }
        val problem = page.error ?: page.blank
        if (problem != null) {
            Surface(
                shape = RoundedCornerShape(24.dp),
                color = MaterialTheme.colorScheme.surfaceContainerHigh,
                shadowElevation = 8.dp,
                modifier = Modifier.align(Alignment.Center).padding(20.dp).fillMaxWidth(),
            ) {
                Column(Modifier.padding(20.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
                    Text(
                        if (page.error != null) "디스코드에 연결하지 못했어요" else "디스코드 화면이 비어 있어요",
                        style = MaterialTheme.typography.titleMedium, fontWeight = FontWeight.Bold,
                    )
                    Text(problem, style = MaterialTheme.typography.bodySmall)
                    Text(
                        "주소: ${page.url.take(80)}\n브라우저: ${UA_NAMES[prefs.uaMode]}" +
                            (if (prefs.safeMode) " · 안전 모드" else ""),
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                    if (page.console.isNotEmpty()) {
                        Text(
                            page.console.joinToString("\n"),
                            style = MaterialTheme.typography.labelSmall.copy(fontFamily = FontFamily.Monospace),
                            color = MaterialTheme.colorScheme.error,
                            maxLines = 8,
                        )
                    }
                    FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                        Button(onClick = { activity.retry() }) { Text("다시 시도") }
                        OutlinedButton(onClick = { activity.retry(safe = !prefs.safeMode) }) {
                            Text(if (prefs.safeMode) "테마 다시 켜기" else "테마 끄고 다시")
                        }
                        OutlinedButton(onClick = { activity.retry(nextUa = true) }) {
                            Text("${UA_NAMES[(prefs.uaMode + 1) % 3]}로 바꾸기")
                        }
                        TextButton(onClick = { activity.openInBrowser() }) { Text("크롬에서 열기") }
                    }
                }
            }
        }
    }
}

@Composable
private fun ThemeChip(t: LumenTheme, selected: Boolean, onClick: () -> Unit) {
    val original = t.id == "original"
    Column(horizontalAlignment = Alignment.CenterHorizontally, modifier = Modifier.clickable(onClick = onClick)) {
        Box(
            Modifier
                .size(56.dp)
                .clip(RoundedCornerShape(16.dp))
                .background(Color(if (original) 0xFF313338 else t.chat))
                .border(
                    if (selected) 3.dp else 1.dp,
                    if (selected) MaterialTheme.colorScheme.primary else Color.Gray.copy(alpha = 0.4f),
                    RoundedCornerShape(16.dp),
                ),
        ) {
            Box(
                Modifier.align(Alignment.CenterStart).width(14.dp).height(56.dp)
                    .background(Color(if (original) 0xFF1E1F22 else t.base))
            )
            Box(
                Modifier.align(Alignment.BottomEnd).padding(8.dp).size(16.dp).clip(CircleShape)
                    .background(Color(if (original) 0xFF5865F2 else t.accent))
            )
        }
        Spacer(Modifier.height(4.dp))
        Text(t.name, style = MaterialTheme.typography.labelSmall, textAlign = TextAlign.Center)
    }
}

@Composable
private fun Toggle(title: String, subtitle: String?, checked: Boolean, onChange: (Boolean) -> Unit) {
    Row(verticalAlignment = Alignment.CenterVertically, modifier = Modifier.fillMaxWidth().clickable { onChange(!checked) }) {
        Column(Modifier.weight(1f)) {
            Text(title, style = MaterialTheme.typography.bodyLarge)
            if (subtitle != null) {
                Text(subtitle, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
            }
        }
        Switch(checked = checked, onCheckedChange = onChange)
    }
}

@Composable
private fun LabeledSlider(
    label: String,
    value: Float,
    range: ClosedFloatingPointRange<Float>,
    steps: Int,
    onDone: (Float) -> Unit,
) {
    var v by remember(value) { mutableFloatStateOf(value) }
    Column {
        Text(label, style = MaterialTheme.typography.bodyMedium)
        Slider(value = v, onValueChange = { v = it }, valueRange = range, steps = steps, onValueChangeFinished = { onDone(v) })
    }
}

/** 켜질 때 새 버전을 확인하고, 받아 두면 위에 설치 카드. */
@Composable
private fun UpdateBanner(modifier: Modifier = Modifier) {
    val context = androidx.compose.ui.platform.LocalContext.current
    val updater = remember { Updater.get(context) }
    val state by updater.state.collectAsStateWithLifecycle()
    LaunchedEffect(Unit) { updater.check() }
    val s = state
    if (s !is Updater.State.Ready && s !is Updater.State.Downloading) return
    Surface(
        shape = RoundedCornerShape(20.dp),
        color = MaterialTheme.colorScheme.primaryContainer,
        shadowElevation = 6.dp,
        modifier = modifier.statusBarsPadding().padding(12.dp).fillMaxWidth(),
    ) {
        Row(Modifier.padding(start = 16.dp, end = 8.dp, top = 10.dp, bottom = 10.dp), verticalAlignment = Alignment.CenterVertically) {
            Icon(Icons.Outlined.Info, null)
            Column(Modifier.weight(1f).padding(horizontal = 12.dp)) {
                when (s) {
                    is Updater.State.Downloading -> {
                        Text("새 버전 ${s.versionName} 받는 중", fontWeight = FontWeight.SemiBold)
                        LinearProgressIndicator(progress = { s.progress }, Modifier.fillMaxWidth().padding(top = 6.dp))
                    }
                    is Updater.State.Ready -> Text("새 버전 ${s.versionName} 준비됨", fontWeight = FontWeight.SemiBold)
                    else -> Unit
                }
            }
            if (s is Updater.State.Ready) {
                TextButton(onClick = { updater.dismiss() }) { Text("나중에") }
                Button(onClick = { updater.install(context) }) { Text("설치") }
            }
        }
    }
}
