package com.dangu.modes

import android.app.ActivityManager
import android.app.ActivityOptions
import android.app.admin.DevicePolicyManager
import android.content.Context
import android.content.Intent
import android.os.Bundle
import android.os.Handler
import android.os.Looper
import android.provider.Settings
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.compose.runtime.mutableStateOf
import com.dangu.modes.ui.PinScreen
import com.dangu.modes.ui.ModesTheme

/**
 * 앱 고정.
 *
 * - Device Owner면: 대상 앱을 잠금 작업 허용 목록에 넣고, 대상 앱을 잠금 상태로 바로 연다
 *   (확인 창 없음, 홈·최근 앱 막힘 — 진짜 키오스크).
 * - 일반 폰이면: 이 액티비티의 태스크를 화면 고정(startLockTask → 시스템 확인 창)하고, 고정이
 *   걸리면 대상 앱을 **같은 태스크 안에**(NEW_TASK 없이) 연다. 고정된 태스크 밖으로는 못 나가므로
 *   대상 앱에서 뒤로 가면 이 화면으로 돌아오고, 여기서만 고정을 풀 수 있다.
 */
class PinHostActivity : ComponentActivity() {
    enum class Phase { Asking, Pinned, Locked, NotEnabled, Blocked, Released }

    val phase = mutableStateOf(Phase.Asking)
    lateinit var target: String
    private val handler = Handler(Looper.getMainLooper())
    private var launchedAt = 0L
    private var pausedSinceLaunch = false
    private var waitStarted = 0L

    private val am get() = getSystemService(ActivityManager::class.java)
    private val lockState get() = am.lockTaskModeState

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        target = intent.getStringExtra(EXTRA_PACKAGE).orEmpty()
        setContent { ModesTheme { PinScreen(this) } }
        if (savedInstanceState == null) begin()
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        intent.getStringExtra(EXTRA_PACKAGE)?.let { if (lockState == ActivityManager.LOCK_TASK_MODE_NONE) { target = it; begin() } }
    }

    private fun begin() {
        if (AdminReceiver.isDeviceOwner(this)) {
            val dpm = getSystemService(DevicePolicyManager::class.java)
            dpm.setLockTaskPackages(AdminReceiver.component(this), arrayOf(packageName, target))
            // 우리 태스크도 잠그고(앱에서 뒤로 오면 여기), 대상 앱은 잠금 상태로 연다.
            startLockTask()
            phase.value = Phase.Locked
            openTarget(lockTaskEnabled = true)
            return
        }
        waitStarted = System.currentTimeMillis()
        phase.value = Phase.Asking
        // 시스템이 "앱을 고정할까요?"를 묻는다. 설정에서 "앱 고정"이 꺼져 있으면 아무 일도 없다.
        startLockTask()
        handler.post(waitForPin)
    }

    /** 고정이 걸렸는지 지켜보다가 걸리면 대상 앱을 연다. */
    private val waitForPin = object : Runnable {
        override fun run() {
            when {
                lockState == ActivityManager.LOCK_TASK_MODE_PINNED -> {
                    phase.value = Phase.Pinned
                    openTarget(lockTaskEnabled = false)
                }
                System.currentTimeMillis() - waitStarted > 20_000 -> {
                    // 확인 창이 안 떴거나(기능 꺼짐) 사용자가 거절했다.
                    phase.value = Phase.NotEnabled
                }
                else -> handler.postDelayed(this, 300)
            }
        }
    }

    fun openTarget(lockTaskEnabled: Boolean = lockState == ActivityManager.LOCK_TASK_MODE_LOCKED) {
        val launch = packageManager.getLaunchIntentForPackage(target) ?: run {
            phase.value = Phase.Blocked
            return
        }
        launchedAt = System.currentTimeMillis()
        pausedSinceLaunch = false
        // 대상 앱이 떴다면 이 화면은 곧 멈춘다(onPause). 1.5초가 지나도 그대로면 시스템이 막은 것 —
        // 첫 화면이 singleTask/singleInstance라 고정된 태스크 안에서 열 수 없는 앱이다.
        handler.postDelayed({
            if (!pausedSinceLaunch && phase.value == Phase.Pinned) phase.value = Phase.Blocked
        }, 1500)
        if (lockTaskEnabled) {
            val opts = ActivityOptions.makeBasic().setLockTaskEnabled(true)
            startActivity(launch.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK), opts.toBundle())
        } else {
            // 같은 태스크 안으로. NEW_TASK를 빼야 고정된 태스크 밖으로 나가지 않는다.
            launch.flags = launch.flags and Intent.FLAG_ACTIVITY_NEW_TASK.inv()
            runCatching { startActivity(launch) }.onFailure { phase.value = Phase.Blocked }
        }
    }

    override fun onPause() {
        super.onPause()
        pausedSinceLaunch = true
    }

    override fun onResume() {
        super.onResume()
        if (phase.value == Phase.Released && lockState != ActivityManager.LOCK_TASK_MODE_NONE) {
            phase.value = if (lockState == ActivityManager.LOCK_TASK_MODE_LOCKED) Phase.Locked else Phase.Pinned
        }
    }

    fun release() {
        runCatching { stopLockTask() }
        if (AdminReceiver.isDeviceOwner(this)) {
            getSystemService(DevicePolicyManager::class.java)
                .setLockTaskPackages(AdminReceiver.component(this), emptyArray())
        }
        phase.value = Phase.Released
        handler.removeCallbacks(waitForPin)
    }

    fun openPinSettings() {
        runCatching { startActivity(Intent(Settings.ACTION_SECURITY_SETTINGS)) }
    }

    override fun onDestroy() {
        handler.removeCallbacks(waitForPin)
        super.onDestroy()
    }

    companion object {
        const val EXTRA_PACKAGE = "package"
        fun intent(context: Context, pkg: String) =
            Intent(context, PinHostActivity::class.java).putExtra(EXTRA_PACKAGE, pkg)
    }
}
