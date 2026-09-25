package com.dangu.modes.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.systemBarsPadding
import androidx.compose.material3.Button
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.dangu.modes.Apps
import com.dangu.modes.PinHostActivity
import com.dangu.modes.PinHostActivity.Phase

/** 고정 호스트 화면. 대상 앱에서 뒤로 오면 여기 — 고정은 여기서만 풀린다. */
@Composable
fun PinScreen(activity: PinHostActivity) {
    val phase by activity.phase
    val name = Apps.label(activity, activity.target)
    Column(
        Modifier.fillMaxSize().background(MaterialTheme.colorScheme.background).systemBarsPadding().padding(32.dp),
        verticalArrangement = Arrangement.Center,
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        val (emoji, title, body) = when (phase) {
            Phase.Asking -> Triple("📌", "앱 고정 확인", "시스템 창에서 \"고정\"을 눌러 주세요.")
            Phase.Pinned -> Triple("📌", "$name 고정 중", "뒤로 가면 이 화면으로 돌아와요. 다른 앱·홈으로는 나갈 수 없어요.")
            Phase.Locked -> Triple("🔐", "$name 잠금 중", "기기 소유자 모드 — 확인 없이 잠겼어요. 여기서만 풀 수 있어요.")
            Phase.NotEnabled -> Triple("⚠️", "화면 고정이 꺼져 있어요", "설정 → 보안 → 앱 고정(화면 고정)을 켠 뒤 다시 시도해 주세요.")
            Phase.Blocked -> Triple("🚫", "이 앱은 고정 화면 안에서 열 수 없어요", "$name 은(는) 자기만의 태스크로 열리는 앱이라 시스템이 막았어요. 고정을 풀어 주세요.")
            Phase.Released -> Triple("✅", "고정 해제됨", "")
        }
        Text(emoji, fontSize = 48.sp)
        Spacer(Modifier.height(12.dp))
        Text(title, style = MaterialTheme.typography.headlineSmall, fontWeight = FontWeight.Bold, textAlign = TextAlign.Center)
        if (body.isNotEmpty()) {
            Spacer(Modifier.height(8.dp))
            Text(body, textAlign = TextAlign.Center, color = MaterialTheme.colorScheme.onSurfaceVariant)
        }
        Spacer(Modifier.height(28.dp))
        when (phase) {
            Phase.Pinned, Phase.Locked -> {
                Button(onClick = { activity.openTarget() }) { Text("$name 다시 열기") }
                Spacer(Modifier.height(8.dp))
                OutlinedButton(onClick = { activity.release() }) { Text("고정 해제") }
            }
            Phase.NotEnabled -> {
                Button(onClick = { activity.openPinSettings() }) { Text("설정 열기") }
                Spacer(Modifier.height(8.dp))
                OutlinedButton(onClick = { activity.recreate() }) { Text("다시 시도") }
            }
            Phase.Blocked -> OutlinedButton(onClick = { activity.release() }) { Text("고정 해제") }
            Phase.Released -> Button(onClick = { activity.finish() }) { Text("닫기") }
            Phase.Asking -> Unit
        }
    }
}
