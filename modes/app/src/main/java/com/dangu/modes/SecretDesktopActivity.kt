package com.dangu.modes

import android.os.Bundle
import android.view.WindowManager
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.biometric.BiometricManager.Authenticators.BIOMETRIC_WEAK
import androidx.biometric.BiometricManager.Authenticators.DEVICE_CREDENTIAL
import androidx.biometric.BiometricPrompt
import androidx.compose.runtime.mutableStateOf
import androidx.core.content.ContextCompat
import androidx.fragment.app.FragmentActivity
import com.dangu.modes.ui.DesktopScreen
import com.dangu.modes.ui.ModesTheme

/**
 * 비밀 바탕화면. 홈이 아니라 "홈 위에 뜨는 전체 화면"이다.
 *
 * - 자기 태스크(taskAffinity)·singleTask: 몇 번을 오가도 하나만 있고 즉시 뜬다.
 * - 최근 앱 목록에서 빠지고(excludeFromRecents) 스크린샷·화면 녹화에도 안 찍힌다(FLAG_SECURE).
 * - 실제 배경화면 위에 그린다(windowShowWallpaper) — 진짜 홈처럼 보인다.
 * - 홈 버튼을 누르면 시스템은 사용자의 원래 홈으로 간다. 기본 홈은 절대 바꾸지 않는다.
 */
class SecretDesktopActivity : FragmentActivity() {
    val unlocked = mutableStateOf(false)

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        window.addFlags(WindowManager.LayoutParams.FLAG_SECURE)
        enableEdgeToEdge()
        setContent { ModesTheme { DesktopScreen(this, secret = true) } }
    }

    override fun onResume() {
        super.onResume()
        if (!Store.get(this).value.secretLock) unlocked.value = true
        else if (!unlocked.value) authenticate()
    }

    override fun onStop() {
        super.onStop()
        // 떠나면 다시 잠근다.
        if (Store.get(this).value.secretLock) unlocked.value = false
    }

    fun authenticate() {
        val prompt = BiometricPrompt(this, ContextCompat.getMainExecutor(this), object : BiometricPrompt.AuthenticationCallback() {
            override fun onAuthenticationSucceeded(result: BiometricPrompt.AuthenticationResult) {
                unlocked.value = true
            }
        })
        runCatching {
            prompt.authenticate(
                BiometricPrompt.PromptInfo.Builder()
                    .setTitle("비밀 바탕화면")
                    .setSubtitle("본인 확인")
                    .setAllowedAuthenticators(BIOMETRIC_WEAK or DEVICE_CREDENTIAL)
                    .build()
            )
        }
    }
}
