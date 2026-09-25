@file:OptIn(androidx.compose.foundation.layout.ExperimentalLayoutApi::class)

package com.dangu.modes.ui

import android.Manifest
import android.content.Intent
import android.net.Uri
import android.os.Build
import android.provider.Settings
import androidx.activity.ComponentActivity
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.detectDragGestures
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.systemBarsPadding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.FilterChip
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Slider
import androidx.compose.material3.Surface
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.produceState
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.shadow
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.ui.zIndex
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.dangu.modes.AdminReceiver
import com.dangu.modes.AppInfo
import com.dangu.modes.Apps
import com.dangu.modes.Mode
import com.dangu.modes.ModeLauncher
import com.dangu.modes.ModeType
import com.dangu.modes.OverlayService
import com.dangu.modes.Store
import kotlin.math.roundToInt
import kotlinx.coroutines.launch

/** 설정. 슬라이더를 움직이면 떠 있는 버튼이 그 자리에서 바로 따라 움직인다. */
@Composable
fun SettingsScreen(activity: ComponentActivity, overlayAllowed: Boolean, onRefresh: () -> Unit) {
    val store = remember { Store.get(activity) }
    val s by store.state.collectAsStateWithLifecycle()
    var editing by remember { mutableStateOf<Mode?>(null) }
    val notif = rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) { onRefresh() }

    Column(
        Modifier
            .fillMaxSize()
            .background(MaterialTheme.colorScheme.background)
            .systemBarsPadding()
            .verticalScroll(rememberScrollState())
            .padding(20.dp),
        verticalArrangement = Arrangement.spacedBy(14.dp),
    ) {
        Text("모드 스위치", style = MaterialTheme.typography.headlineMedium, fontWeight = FontWeight.Bold)
        Text(
            "오른쪽 끝의 버튼으로 바탕화면·앱 고정 모드를 바로 바꿉니다. 기본 홈 앱은 바꾸지 않아요.",
            color = MaterialTheme.colorScheme.onSurfaceVariant, style = MaterialTheme.typography.bodyMedium,
        )

        // ── 권한과 켜기 ──
        Card {
            if (!overlayAllowed) {
                Text("다른 앱 위에 표시 권한이 필요해요", fontWeight = FontWeight.SemiBold)
                Text("버튼과 모드 목록을 모든 화면 위에 띄우는 데만 씁니다.", color = MaterialTheme.colorScheme.onSurfaceVariant, fontSize = 13.sp)
                Button(onClick = {
                    activity.startActivity(
                        Intent(Settings.ACTION_MANAGE_OVERLAY_PERMISSION, Uri.parse("package:${activity.packageName}"))
                    )
                }) { Text("권한 허용하러 가기") }
            }
            Toggle("플로팅 버튼", if (s.enabled) "켜짐 — 모든 화면 오른쪽 끝" else "꺼짐", s.enabled) { on ->
                if (on) {
                    if (Build.VERSION.SDK_INT >= 33) notif.launch(Manifest.permission.POST_NOTIFICATIONS)
                    store.update { copy(enabled = true) }
                    if (!OverlayService.start(activity)) {
                        store.update { copy(enabled = false) }
                        activity.startActivity(Intent(Settings.ACTION_MANAGE_OVERLAY_PERMISSION, Uri.parse("package:${activity.packageName}")))
                    }
                } else {
                    store.update { copy(enabled = false) }
                    OverlayService.stop(activity)
                }
            }
        }

        // ── 버튼 모양 ──
        Card {
            Section("버튼")
            LabeledSlider("세로 위치 ${(s.y * 100).roundToInt()}%", s.y, 0f..1f) { v -> store.update { copy(y = v) } }
            LabeledSlider("크기 ${s.sizeDp}dp", s.sizeDp.toFloat(), 28f..96f) { v -> store.update { copy(sizeDp = v.roundToInt()) } }
            LabeledSlider("투명도 ${(s.alpha * 100).roundToInt()}%", s.alpha, 0.2f..1f) { v -> store.update { copy(alpha = v) } }
            Text("버튼을 직접 위아래로 끌어도 됩니다.", color = MaterialTheme.colorScheme.onSurfaceVariant, fontSize = 12.sp)
        }

        // ── 모드 목록 ──
        Card {
            Section("모드 목록")
            Text("≡ 를 잡고 끌어 순서를 바꿉니다. 팝업에 이 순서대로 나와요.", color = MaterialTheme.colorScheme.onSurfaceVariant, fontSize = 12.sp)
            ReorderableModes(
                modes = s.modes,
                onMove = { from, to ->
                    store.update {
                        copy(modes = modes.toMutableList().apply { add(to, removeAt(from)) })
                    }
                },
                onEdit = { editing = it },
                onTry = { ModeLauncher.launch(activity, it) },
            )
            OutlinedButton(onClick = { editing = Mode(Mode.newId(), ModeType.LAUNCH, "") }) { Text("+ 모드 추가") }
        }

        // ── 비밀 바탕화면 ──
        Card {
            Section("비밀 바탕화면")
            Toggle("들어갈 때 지문·PIN 확인", "나오면 다시 잠겨요. 최근 앱 목록과 스크린샷에는 항상 안 남아요.", s.secretLock) {
                store.update { copy(secretLock = it) }
            }
            Text(
                if (s.desktop.isEmpty) "아직 텅 비어 있어요." else "앱 ${s.desktop.allApps().size}개가 놓여 있어요.",
                color = MaterialTheme.colorScheme.onSurfaceVariant, fontSize = 13.sp,
            )
            Button(onClick = {
                activity.startActivity(
                    Intent(activity, com.dangu.modes.SecretDesktopActivity::class.java)
                        .putExtra(com.dangu.modes.SecretDesktopActivity.EXTRA_EDIT, true)
                        .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
                )
            }) { Text("바탕화면 편집") }
        }

        // ── 앱 고정 안내 ──
        Card {
            Section("앱 고정")
            val owner = AdminReceiver.isDeviceOwner(activity)
            Text(
                if (owner) "기기 소유자(Device Owner) 모드 — 확인 창 없이 완전히 잠급니다."
                else "일반 모드 — 시스템이 \"앱을 고정할까요?\"를 한 번 묻습니다. 설정 → 보안 → 앱 고정이 켜져 있어야 해요.",
                fontSize = 13.sp,
            )
            if (!owner) {
                Text(
                    "키오스크처럼 확인 없이 잠그려면(공장 초기화 직후, 계정 없을 때):\nadb shell dpm set-device-owner ${activity.packageName}/.AdminReceiver",
                    color = MaterialTheme.colorScheme.onSurfaceVariant, fontSize = 11.sp,
                )
                TextButton(onClick = { activity.startActivity(Intent(Settings.ACTION_SECURITY_SETTINGS)) }) { Text("앱 고정 설정 열기") }
            }
        }
        UpdateCard(activity)
        Spacer(Modifier.height(24.dp))
    }

    editing?.let { m ->
        EditModeDialog(
            activity, m,
            isNew = s.modes.none { it.id == m.id },
            onSave = { saved ->
                store.update {
                    copy(modes = if (modes.any { it.id == saved.id }) modes.map { if (it.id == saved.id) saved else it } else modes + saved)
                }
                editing = null
            },
            onDelete = {
                store.update { copy(modes = modes.filter { it.id != m.id }) }
                editing = null
            },
            onDismiss = { editing = null },
        )
    }
}

/**
 * 끌어서 순서 바꾸기. 줄 높이가 같으므로, 끈 거리가 반 줄을 넘을 때마다 이웃과 자리를 바꾼다.
 * 끄는 동안 그 줄만 손가락을 따라 떠 있게 그린다.
 */
@Composable
private fun ReorderableModes(
    modes: List<Mode>,
    onMove: (from: Int, to: Int) -> Unit,
    onEdit: (Mode) -> Unit,
    onTry: (Mode) -> Unit,
) {
    val rowHeight = 60.dp
    val rowPx = with(LocalDensity.current) { rowHeight.toPx() }
    var draggingId by remember { mutableStateOf<String?>(null) }
    var offset by remember { mutableFloatStateOf(0f) }
    val current by rememberUpdatedState(modes)
    val move by rememberUpdatedState(onMove)
    val context = androidx.compose.ui.platform.LocalContext.current

    Column {
        modes.forEach { m ->
            key(m.id) {
                val dragging = draggingId == m.id
                Row(
                    Modifier
                        .fillMaxWidth()
                        .height(rowHeight)
                        .zIndex(if (dragging) 1f else 0f)
                        .graphicsLayer { translationY = if (dragging) offset else 0f }
                        .then(if (dragging) Modifier.shadow(8.dp, RoundedCornerShape(12.dp)) else Modifier)
                        .clip(RoundedCornerShape(12.dp))
                        .background(if (dragging) MaterialTheme.colorScheme.surfaceContainerHighest else MaterialTheme.colorScheme.surfaceContainerHigh.copy(alpha = 0f)),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Box(
                        Modifier
                            .size(48.dp)
                            .semantics { contentDescription = "끌어서 순서 바꾸기: ${m.label}" }
                            .pointerInput(m.id) {
                                detectDragGestures(
                                    onDragStart = { draggingId = m.id; offset = 0f },
                                    onDragEnd = { draggingId = null; offset = 0f },
                                    onDragCancel = { draggingId = null; offset = 0f },
                                ) { change, amount ->
                                    change.consume()
                                    offset += amount.y
                                    val list = current
                                    val i = list.indexOfFirst { it.id == m.id }
                                    if (offset > rowPx / 2 && i < list.lastIndex) {
                                        move(i, i + 1); offset -= rowPx
                                    } else if (offset < -rowPx / 2 && i > 0) {
                                        move(i, i - 1); offset += rowPx
                                    }
                                }
                            },
                        contentAlignment = Alignment.Center,
                    ) { Text("≡", fontSize = 22.sp, color = MaterialTheme.colorScheme.onSurfaceVariant) }
                    Text(m.type.emoji, fontSize = 20.sp)
                    Spacer(Modifier.width(10.dp))
                    Column(Modifier.weight(1f)) {
                        Text(m.label, maxLines = 1, overflow = TextOverflow.Ellipsis)
                        val sub = if (m.needsApp) {
                            if (m.packageName.isBlank()) "앱을 골라 주세요" else "${m.type.label} · ${Apps.label(context, m.packageName)}"
                        } else m.type.label
                        Text(sub, color = MaterialTheme.colorScheme.onSurfaceVariant, fontSize = 12.sp, maxLines = 1, overflow = TextOverflow.Ellipsis)
                    }
                    TextButton(onClick = { onTry(m) }) { Text("해 보기") }
                    TextButton(onClick = { onEdit(m) }) { Text("편집") }
                }
            }
        }
    }
}

@Composable
private fun EditModeDialog(
    activity: ComponentActivity,
    mode: Mode,
    isNew: Boolean,
    onSave: (Mode) -> Unit,
    onDelete: () -> Unit,
    onDismiss: () -> Unit,
) {
    var type by remember { mutableStateOf(mode.type) }
    var label by remember { mutableStateOf(mode.label) }
    var pkg by remember { mutableStateOf(mode.packageName) }
    var pickingApp by remember { mutableStateOf(false) }

    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text(if (isNew) "모드 추가" else "모드 편집") },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
                Text("종류", fontSize = 13.sp)
                androidx.compose.foundation.layout.FlowRow(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                    ModeType.entries.forEach { t ->
                        FilterChip(selected = type == t, onClick = { type = t }, label = { Text("${t.emoji} ${t.label}") })
                    }
                }
                OutlinedTextField(label, { label = it }, label = { Text("이름") }, singleLine = true)
                if (type == ModeType.LAUNCH || type == ModeType.PIN) {
                    Row(verticalAlignment = Alignment.CenterVertically) {
                        Text(if (pkg.isBlank()) "앱 없음" else Apps.label(activity, pkg), Modifier.weight(1f), maxLines = 1)
                        TextButton(onClick = { pickingApp = true }) { Text("앱 고르기") }
                    }
                }
            }
        },
        confirmButton = {
            TextButton(onClick = {
                val name = label.ifBlank { if (pkg.isNotBlank()) "${Apps.label(activity, pkg)} ${if (type == ModeType.PIN) "고정" else ""}".trim() else type.label }
                onSave(mode.copy(type = type, label = name, packageName = if (type == ModeType.LAUNCH || type == ModeType.PIN) pkg else ""))
            }) { Text("저장") }
        },
        dismissButton = {
            Row {
                if (!isNew) TextButton(onClick = onDelete) { Text("삭제", color = MaterialTheme.colorScheme.error) }
                TextButton(onClick = onDismiss) { Text("취소") }
            }
        },
    )
    if (pickingApp) {
        AppPickerDialog(activity, onPick = { pkg = it; pickingApp = false }, onDismiss = { pickingApp = false })
    }
}

@Composable
private fun AppPickerDialog(activity: ComponentActivity, onPick: (String) -> Unit, onDismiss: () -> Unit) {
    val apps by produceState<List<AppInfo>?>(null) { value = Apps.launchable(activity) }
    var query by remember { mutableStateOf("") }
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text("앱 고르기") },
        text = {
            Column {
                OutlinedTextField(query, { query = it }, placeholder = { Text("검색") }, singleLine = true)
                Spacer(Modifier.height(8.dp))
                val list = apps?.filter { query.isBlank() || it.label.contains(query, true) || it.packageName.contains(query, true) }
                if (list == null) Text("불러오는 중…")
                else LazyColumn(Modifier.heightIn(max = 360.dp)) {
                    items(list, key = { it.packageName }) { a -> AppRow(a) { onPick(a.packageName) } }
                }
            }
        },
        confirmButton = { TextButton(onClick = onDismiss) { Text("닫기") } },
    )
}

@Composable
private fun AppRow(a: AppInfo, onClick: () -> Unit) {
    Row(Modifier.fillMaxWidth().clickable(onClick = onClick).padding(vertical = 6.dp), verticalAlignment = Alignment.CenterVertically) {
        if (a.icon != null) Image(a.icon.asImageBitmap(), null, Modifier.size(32.dp))
        Spacer(Modifier.width(12.dp))
        Text(a.label, maxLines = 1, overflow = TextOverflow.Ellipsis)
    }
}

@Composable
private fun Card(content: @Composable () -> Unit) {
    Surface(shape = RoundedCornerShape(20.dp), color = MaterialTheme.colorScheme.surfaceContainer, modifier = Modifier.fillMaxWidth()) {
        Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) { content() }
    }
}

@Composable
private fun Section(title: String) {
    Text(title, style = MaterialTheme.typography.titleMedium, fontWeight = FontWeight.SemiBold)
}

@Composable
private fun Toggle(title: String, subtitle: String, checked: Boolean, onChange: (Boolean) -> Unit) {
    Row(Modifier.fillMaxWidth().clickable { onChange(!checked) }, verticalAlignment = Alignment.CenterVertically) {
        Column(Modifier.weight(1f)) {
            Text(title, style = MaterialTheme.typography.bodyLarge)
            Text(subtitle, color = MaterialTheme.colorScheme.onSurfaceVariant, fontSize = 12.sp)
        }
        Switch(checked, onChange)
    }
}

@Composable
private fun LabeledSlider(label: String, value: Float, range: ClosedFloatingPointRange<Float>, onChange: (Float) -> Unit) {
    Column {
        Text(label, fontSize = 14.sp)
        // 움직이는 동안 계속 반영 — 떠 있는 버튼이 실시간으로 따라온다.
        Slider(value = value, onValueChange = onChange, valueRange = range)
    }
}

/** 자동 업데이트 상태. 새 버전을 받아 두었으면 설치 단추. */
@Composable
private fun UpdateCard(activity: ComponentActivity) {
    val updater = remember { com.dangu.modes.Updater.get(activity) }
    val state by updater.state.collectAsStateWithLifecycle()
    val scope = androidx.compose.runtime.rememberCoroutineScope()
    Card {
        Section("업데이트")
        Row(verticalAlignment = Alignment.CenterVertically) {
            Column(Modifier.weight(1f)) {
                Text("지금 ${updater.currentVersion}", fontSize = 14.sp)
                Text(
                    when (val u = state) {
                        com.dangu.modes.Updater.State.Checking -> "확인 중…"
                        com.dangu.modes.Updater.State.UpToDate -> "최신 버전이에요"
                        is com.dangu.modes.Updater.State.Downloading -> "새 버전 ${u.versionName} 받는 중 ${(u.progress * 100).roundToInt()}%"
                        is com.dangu.modes.Updater.State.Ready -> "새 버전 ${u.versionName} 준비됨"
                        is com.dangu.modes.Updater.State.Failed -> "확인 실패: ${u.message}"
                        else -> "앱을 열 때와 버튼이 떠 있는 동안 반나절마다 알아서 확인해요"
                    },
                    color = MaterialTheme.colorScheme.onSurfaceVariant, fontSize = 12.sp,
                )
            }
            if (state is com.dangu.modes.Updater.State.Ready) {
                Button(onClick = { updater.install(activity) }) { Text("설치") }
            } else {
                TextButton(onClick = { scope.launch { com.dangu.modes.AutoUpdate.run(activity, force = true) } }) { Text("지금 확인") }
            }
        }
    }
}
