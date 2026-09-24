package com.dangu.gallery.ui

import android.Manifest
import android.content.Context
import android.content.Intent
import android.net.Uri
import android.provider.Settings
import android.content.pm.PackageManager
import android.os.Build
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.PhotoLibrary
import androidx.compose.material3.Button
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.core.content.ContextCompat
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.compose.LifecycleEventEffect

enum class MediaAccess { None, Partial, Full }

private fun granted(context: Context, permission: String) =
    ContextCompat.checkSelfPermission(context, permission) == PackageManager.PERMISSION_GRANTED

fun mediaAccess(context: Context): MediaAccess = when {
    Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU &&
        granted(context, Manifest.permission.READ_MEDIA_IMAGES) -> MediaAccess.Full
    Build.VERSION.SDK_INT >= Build.VERSION_CODES.UPSIDE_DOWN_CAKE &&
        granted(context, Manifest.permission.READ_MEDIA_VISUAL_USER_SELECTED) -> MediaAccess.Partial
    Build.VERSION.SDK_INT < Build.VERSION_CODES.TIRAMISU &&
        granted(context, Manifest.permission.READ_EXTERNAL_STORAGE) -> MediaAccess.Full
    else -> MediaAccess.None
}

fun mediaPermissions(): Array<String> = buildList {
    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
        add(Manifest.permission.READ_MEDIA_IMAGES)
        add(Manifest.permission.READ_MEDIA_VIDEO)
    } else {
        add(Manifest.permission.READ_EXTERNAL_STORAGE)
    }
    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.UPSIDE_DOWN_CAKE) {
        add(Manifest.permission.READ_MEDIA_VISUAL_USER_SELECTED)
    }
    add(Manifest.permission.ACCESS_MEDIA_LOCATION)
}.toTypedArray()

/**
 * 권한을 확인하고, 없으면 묻는다. 설정에서 바꾸고 돌아와도 알아채도록 화면에 돌아올 때마다 다시 본다.
 * [content]는 접근 수준을 받아 그린다 — Partial이면 "전체 허용" 안내를 띄울 수 있게.
 */
@Composable
fun WithMediaPermission(
    askOnStart: Boolean = true,
    content: @Composable (access: MediaAccess, requestAgain: () -> Unit) -> Unit,
) {
    val context = LocalContext.current
    var access by remember { mutableStateOf(mediaAccess(context)) }
    val launcher = rememberLauncherForActivityResult(
        ActivityResultContracts.RequestMultiplePermissions()
    ) { access = mediaAccess(context) }
    val request = { launcher.launch(mediaPermissions()) }

    LifecycleEventEffect(Lifecycle.Event.ON_RESUME) { access = mediaAccess(context) }
    LaunchedEffect(Unit) { if (askOnStart && access == MediaAccess.None) request() }

    if (access == MediaAccess.None) {
        Column(
            Modifier.fillMaxSize().padding(32.dp),
            verticalArrangement = Arrangement.Center,
            horizontalAlignment = Alignment.CenterHorizontally,
        ) {
            Icon(Icons.Outlined.PhotoLibrary, null, Modifier.size(56.dp), tint = MaterialTheme.colorScheme.primary)
            Spacer(Modifier.height(16.dp))
            Text("사진과 동영상에 접근하도록 허용해 주세요", style = MaterialTheme.typography.titleMedium, textAlign = TextAlign.Center)
            Spacer(Modifier.height(8.dp))
            Text(
                "기기에 있는 사진을 보여 주고 파일 정보를 읽는 데만 씁니다. 어디로도 보내지 않습니다.",
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                textAlign = TextAlign.Center,
            )
            Spacer(Modifier.height(24.dp))
            Button(onClick = request) { Text("허용하기") }
            TextButton(onClick = {
                context.startActivity(
                    Intent(Settings.ACTION_APPLICATION_DETAILS_SETTINGS, Uri.fromParts("package", context.packageName, null))
                        .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
                )
            }) { Text("설정에서 허용") }
        }
    } else {
        content(access, request)
    }
}
