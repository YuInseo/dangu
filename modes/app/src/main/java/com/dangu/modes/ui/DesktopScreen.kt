package com.dangu.modes.ui

import android.content.Intent
import androidx.activity.ComponentActivity
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.systemBarsPadding
import androidx.compose.foundation.lazy.grid.GridCells
import androidx.compose.foundation.lazy.grid.LazyVerticalGrid
import androidx.compose.foundation.lazy.grid.items
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Button
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.produceState
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.dangu.modes.Apps
import com.dangu.modes.MainActivity
import com.dangu.modes.SecretDesktopActivity
import com.dangu.modes.Store
import com.dangu.modes.WorkProfile
import kotlinx.coroutines.delay
import java.time.LocalTime
import java.time.format.DateTimeFormatter

private data class Tile(val key: String, val label: String, val icon: android.graphics.Bitmap?, val open: () -> Unit)

/** 비밀 바탕화면 / 업무 프로필 바탕화면. 실제 배경화면 위에 그린다. */
@Composable
fun DesktopScreen(activity: ComponentActivity, secret: Boolean) {
    val store = remember { Store.get(activity) }
    val s by store.state.collectAsStateWithLifecycle()
    val unlocked = (activity as? SecretDesktopActivity)?.unlocked?.value ?: true

    Box(Modifier.fillMaxSize().background(Color.Black.copy(alpha = if (secret) 0.55f else 0.35f)).systemBarsPadding()) {
        if (secret && !unlocked) {
            Column(Modifier.align(Alignment.Center), horizontalAlignment = Alignment.CenterHorizontally) {
                Text("🔒", fontSize = 56.sp)
                Spacer(Modifier.height(12.dp))
                Text("비밀 바탕화면이 잠겨 있어요", color = Color.White)
                Spacer(Modifier.height(16.dp))
                Button(onClick = { (activity as SecretDesktopActivity).authenticate() }) { Text("잠금 해제") }
            }
        } else {
            DesktopApps(activity, secret, s.secretApps)
        }
    }
}

@Composable
private fun DesktopApps(activity: ComponentActivity, secret: Boolean, secretApps: List<String>) {
    val tiles by produceState<List<Tile>?>(null, secretApps, secret) {
        value = if (secret) {
            val all = Apps.launchable(activity).associateBy { it.packageName }
            secretApps.mapNotNull { all[it] }.map { a ->
                Tile(a.packageName, a.label, a.icon) {
                    activity.packageManager.getLaunchIntentForPackage(a.packageName)?.let { activity.startActivity(it) }
                }
            }
        } else {
            WorkProfile.apps(activity).map { w ->
                Tile(w.component.flattenToString() + w.user, w.label, w.icon) { WorkProfile.launch(activity, w) }
            }
        }
    }

    Column(Modifier.fillMaxSize()) {
        Clock(if (secret) "비밀 바탕화면" else "업무 프로필")
        val list = tiles
        when {
            list == null -> Unit
            list.isEmpty() -> Column(
                Modifier.fillMaxWidth().weight(1f).padding(32.dp),
                verticalArrangement = Arrangement.Center,
                horizontalAlignment = Alignment.CenterHorizontally,
            ) {
                Text(
                    if (secret) "여기에 둘 앱을 설정에서 골라 주세요."
                    else "업무 프로필이 없어요.\n회사 기기관리(MDM)나 Shelter·Island 같은 앱으로 만들면 여기에 그 앱들이 모여요.",
                    color = Color.White, textAlign = TextAlign.Center,
                )
                if (secret) {
                    Spacer(Modifier.height(16.dp))
                    Button(onClick = { activity.startActivity(Intent(activity, MainActivity::class.java)) }) { Text("설정 열기") }
                }
            }
            else -> LazyVerticalGrid(
                columns = GridCells.Fixed(4),
                modifier = Modifier.weight(1f),
                contentPadding = PaddingValues(16.dp),
                verticalArrangement = Arrangement.spacedBy(20.dp),
            ) {
                items(list, key = { it.key }) { t ->
                    Column(
                        Modifier.clip(RoundedCornerShape(12.dp)).clickable { t.open() }.padding(4.dp),
                        horizontalAlignment = Alignment.CenterHorizontally,
                    ) {
                        if (t.icon != null) Image(t.icon.asImageBitmap(), t.label, Modifier.size(52.dp))
                        else Box(Modifier.size(52.dp).clip(RoundedCornerShape(14.dp)).background(Accent))
                        Spacer(Modifier.height(6.dp))
                        Text(t.label, color = Color.White, fontSize = 12.sp, maxLines = 1, overflow = TextOverflow.Ellipsis)
                    }
                }
            }
        }
        Row(Modifier.fillMaxWidth().padding(12.dp), horizontalArrangement = Arrangement.Center) {
            TextButton(onClick = {
                activity.startActivity(Intent(Intent.ACTION_MAIN).addCategory(Intent.CATEGORY_HOME).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
            }) { Text("🏠  기본 바탕화면으로", color = Color.White) }
        }
    }
}

@Composable
private fun Clock(caption: String) {
    var now by remember { mutableStateOf(LocalTime.now()) }
    LaunchedEffect(Unit) { while (true) { now = LocalTime.now(); delay(10_000) } }
    Column(Modifier.fillMaxWidth().padding(top = 40.dp, bottom = 24.dp), horizontalAlignment = Alignment.CenterHorizontally) {
        Text(now.format(DateTimeFormatter.ofPattern("HH:mm")), color = Color.White, fontSize = 56.sp, fontWeight = FontWeight.Light)
        Text(caption, color = Color.White.copy(alpha = 0.8f), style = MaterialTheme.typography.labelLarge)
    }
}
