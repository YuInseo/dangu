package com.dangu.gallery

import android.content.Intent
import android.net.Uri
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.compose.runtime.mutableStateOf
import androidx.core.content.IntentCompat
import com.dangu.gallery.ui.GalleryTheme
import com.dangu.gallery.ui.InfoPanelScreen

/**
 * 지금 보던 화면 위에 뜨는 사진 정보 패널.
 *
 * - 런처/엣지 패널에서 열면: 가장 최근 사진들을 띄우고 맨 앞 사진의 정보를 바로 보여 준다.
 *   방금 찍은 사진이나 스크린샷을 AI 셀렉트처럼 곧장 확인하는 용도.
 * - 다른 앱에서 "공유 → 사진 정보"로 보내면: 보낸 사진의 정보.
 */
class InfoActivity : ComponentActivity() {
    private val shared = mutableStateOf<List<Uri>>(emptyList())

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()
        shared.value = urisOf(intent)
        setContent {
            GalleryTheme {
                InfoPanelScreen(
                    shared = shared.value,
                    onClose = { finish() },
                    onOpenGallery = {
                        startActivity(
                            Intent(this, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
                        )
                        finish()
                    },
                )
            }
        }
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        setIntent(intent)
        shared.value = urisOf(intent)
    }

    private fun urisOf(intent: Intent?): List<Uri> {
        intent ?: return emptyList()
        val out = LinkedHashSet<Uri>()
        when (intent.action) {
            Intent.ACTION_SEND ->
                IntentCompat.getParcelableExtra(intent, Intent.EXTRA_STREAM, Uri::class.java)?.let(out::add)
            Intent.ACTION_SEND_MULTIPLE ->
                IntentCompat.getParcelableArrayListExtra(intent, Intent.EXTRA_STREAM, Uri::class.java)
                    ?.let(out::addAll)
            Intent.ACTION_VIEW -> intent.data?.let(out::add)
        }
        if (out.isEmpty() && intent.action != Intent.ACTION_MAIN) {
            intent.clipData?.let { clip -> for (i in 0 until clip.itemCount) clip.getItemAt(i).uri?.let(out::add) }
        }
        return out.toList()
    }
}
