package com.dangu.modes

import android.os.Bundle
import android.provider.Settings
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.compose.runtime.mutableStateOf
import com.dangu.modes.ui.ModesTheme
import com.dangu.modes.ui.SettingsScreen

/** 설정 화면. 권한을 받고 돌아오면(onResume) 상태를 다시 본다. */
class MainActivity : ComponentActivity() {
    private val overlayAllowed = mutableStateOf(false)

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()
        setContent {
            ModesTheme {
                SettingsScreen(this, overlayAllowed.value, onRefresh = ::refresh)
            }
        }
    }

    override fun onResume() {
        super.onResume()
        refresh()
        // 권한을 받고 돌아왔는데 버튼이 켜져 있다면 바로 띄운다.
        if (overlayAllowed.value && Store.get(this).value.enabled) OverlayService.start(this)
    }

    private fun refresh() {
        overlayAllowed.value = Settings.canDrawOverlays(this)
    }
}
