package com.dangu.modes

import android.content.Context
import android.content.SharedPreferences
import androidx.core.content.edit
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow

/**
 * 설정 전부. 설정 화면이 쓰고, 플로팅 서비스가 듣는다(같은 프로세스).
 * 값이 바뀌면 [state]가 새 스냅샷을 내보내므로 서비스는 updateViewLayout()만 하면 된다.
 */
class Store private constructor(context: Context) {
    private val sp: SharedPreferences = context.getSharedPreferences("modes", Context.MODE_PRIVATE)

    data class Snapshot(
        val enabled: Boolean,
        /** 버튼 세로 위치, 화면 높이의 비율(0=맨 위, 1=맨 아래) */
        val y: Float,
        /** 버튼 크기(dp) */
        val sizeDp: Int,
        /** 쉴 때 투명도 0.2~1 */
        val alpha: Float,
        val modes: List<Mode>,
        /** 비밀 바탕화면에 놓을 앱 */
        val secretApps: List<String>,
        /** 비밀 바탕화면에 들어갈 때 지문·PIN 확인 */
        val secretLock: Boolean,
    )

    private fun read() = Snapshot(
        enabled = sp.getBoolean("enabled", false),
        y = sp.getFloat("y", 0.4f),
        sizeDp = sp.getInt("size", 48),
        alpha = sp.getFloat("alpha", 0.85f),
        modes = Mode.listFromJson(sp.getString("modes", null)) ?: Mode.defaults,
        secretApps = sp.getString("secretApps", "")!!.split(',').filter { it.isNotBlank() },
        secretLock = sp.getBoolean("secretLock", false),
    )

    private val _state = MutableStateFlow(read())
    val state: StateFlow<Snapshot> = _state.asStateFlow()
    val value get() = _state.value

    fun update(block: Snapshot.() -> Snapshot) {
        val n = value.block().let {
            it.copy(y = it.y.coerceIn(0f, 1f), sizeDp = it.sizeDp.coerceIn(28, 96), alpha = it.alpha.coerceIn(0.2f, 1f))
        }
        sp.edit {
            putBoolean("enabled", n.enabled)
            putFloat("y", n.y)
            putInt("size", n.sizeDp)
            putFloat("alpha", n.alpha)
            putString("modes", Mode.listToJson(n.modes))
            putString("secretApps", n.secretApps.joinToString(","))
            putBoolean("secretLock", n.secretLock)
        }
        _state.value = n
    }

    companion object {
        @Volatile private var instance: Store? = null
        fun get(context: Context): Store = instance ?: synchronized(this) {
            instance ?: Store(context.applicationContext).also { instance = it }
        }
    }
}
