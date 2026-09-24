package com.dangu.gallery.ui

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.SystemUpdate
import androidx.compose.material3.Button
import androidx.compose.material3.Icon
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.dangu.gallery.data.Updater

/**
 * 앱을 켜면 새 버전을 확인하고, 있으면 받아 둔 뒤 위에 "설치" 카드를 띄운다.
 * 확인·다운로드 중이거나 최신이면 아무것도 보이지 않는다.
 */
@Composable
fun UpdateBanner(modifier: Modifier = Modifier) {
    val context = LocalContext.current
    val updater = remember(context) { Updater.get(context) }
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
        Column(Modifier.padding(start = 16.dp, end = 8.dp, top = 10.dp, bottom = 6.dp)) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Icon(Icons.Outlined.SystemUpdate, null)
                Column(Modifier.weight(1f).padding(horizontal = 12.dp)) {
                    when (s) {
                        is Updater.State.Downloading -> {
                            Text("새 버전 ${s.versionName} 받는 중", fontWeight = FontWeight.SemiBold)
                            LinearProgressIndicator(progress = { s.progress }, Modifier.fillMaxWidth().padding(top = 6.dp))
                        }
                        is Updater.State.Ready -> {
                            Text("새 버전 ${s.versionName} 준비됨", fontWeight = FontWeight.SemiBold)
                            Text(
                                "지금 ${updater.currentVersion}",
                                style = MaterialTheme.typography.bodySmall,
                            )
                        }
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
}
