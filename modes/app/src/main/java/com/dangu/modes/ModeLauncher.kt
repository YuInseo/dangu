package com.dangu.modes

import android.app.ActivityManager
import android.content.Context
import android.content.Intent
import android.widget.Toast

/** 모드 하나를 실제 화면 전환으로. 플로팅 팝업과 설정 화면의 "해 보기"가 같이 쓴다. */
object ModeLauncher {
    fun launch(context: Context, mode: Mode) {
        val am = context.getSystemService(ActivityManager::class.java)
        if (am.lockTaskModeState != ActivityManager.LOCK_TASK_MODE_NONE) {
            toast(context, "앱 고정 중에는 모드를 바꿀 수 없어요")
            return
        }
        when (mode.type) {
            // 사용자가 고른 기본 홈으로. 누가 홈인지는 시스템이 정한다 — 우리는 바꾸지 않는다.
            ModeType.HOME -> start(
                context,
                Intent(Intent.ACTION_MAIN).addCategory(Intent.CATEGORY_HOME),
            )
            ModeType.SECRET -> start(context, Intent(context, SecretDesktopActivity::class.java))
            ModeType.WORK -> start(context, Intent(context, WorkDesktopActivity::class.java))
            ModeType.LAUNCH -> {
                val i = context.packageManager.getLaunchIntentForPackage(mode.packageName)
                if (i == null) toast(context, "앱을 찾을 수 없어요: ${mode.packageName}") else start(context, i)
            }
            ModeType.PIN -> {
                if (mode.packageName.isBlank()) toast(context, "고정할 앱을 설정에서 골라 주세요")
                else start(context, PinHostActivity.intent(context, mode.packageName))
            }
        }
    }

    private fun start(context: Context, intent: Intent) {
        runCatching {
            context.startActivity(intent.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_RESET_TASK_IF_NEEDED))
        }.onFailure { toast(context, "열지 못했어요: ${it.message}") }
    }

    private fun toast(context: Context, text: String) = Toast.makeText(context, text, Toast.LENGTH_SHORT).show()
}
