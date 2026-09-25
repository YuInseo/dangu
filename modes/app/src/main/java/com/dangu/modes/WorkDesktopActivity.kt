package com.dangu.modes

import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import com.dangu.modes.ui.DesktopScreen
import com.dangu.modes.ui.ModesTheme

/** 업무 프로필의 앱만 모은 바탕화면. 앱 목록과 실행은 LauncherApps로(다른 프로필이라서). */
class WorkDesktopActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()
        setContent { ModesTheme { DesktopScreen(this, secret = false) } }
    }
}
