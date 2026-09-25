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
        /** 막대 길이(dp) */
        val sizeDp: Int,
        /** 막대 굵기(dp) */
        val thickDp: Int,
        /** 막대 색(ARGB) */
        val color: Int,
        /** 쉴 때 투명도 0.2~1 */
        val alpha: Float,
        /** 버튼이 붙는 쪽: 오른쪽이면 true */
        val right: Boolean,
        /** 원형 메뉴에 한 번에 보일 최대 개수. 넘치면 돌려서 본다. */
        val maxVisible: Int,
        /** 메뉴 모양: true면 엣지 패널처럼 세로 목록, false면 원형 */
        val panel: Boolean,
        val modes: List<Mode>,
        /** 비밀 바탕화면의 배치(처음엔 비어 있다) */
        val desktop: Desktop,
        /** 비밀 바탕화면에 들어갈 때 지문·PIN 확인 */
        val secretLock: Boolean,
    )

    private fun read() = Snapshot(
        enabled = sp.getBoolean("enabled", false),
        y = sp.getFloat("y", 0.4f),
        sizeDp = sp.getInt("barLength", 72),
        thickDp = sp.getInt("thick", 6),
        color = sp.getInt("color", 0xFFFFFFFF.toInt()),
        alpha = sp.getFloat("alpha", 0.85f),
        right = sp.getBoolean("right", true),
        maxVisible = sp.getInt("maxVisible", 5),
        panel = sp.getBoolean("panel", true),
        modes = Mode.listFromJson(sp.getString("modes", null)) ?: Mode.defaults,
        desktop = Desktop.fromJson(sp.getString("desktop", null)),
        secretLock = sp.getBoolean("secretLock", false),
    )

    private val _state = MutableStateFlow(read())
    val state: StateFlow<Snapshot> = _state.asStateFlow()
    val value get() = _state.value

    fun update(block: Snapshot.() -> Snapshot) {
        val n = value.block().let {
            it.copy(
                y = it.y.coerceIn(0f, 1f), sizeDp = it.sizeDp.coerceIn(32, 200), thickDp = it.thickDp.coerceIn(3, 16), alpha = it.alpha.coerceIn(0.2f, 1f),
                maxVisible = it.maxVisible.coerceIn(3, 9),
            )
        }
        sp.edit {
            putBoolean("enabled", n.enabled)
            putFloat("y", n.y)
            putInt("barLength", n.sizeDp)
            putInt("thick", n.thickDp)
            putInt("color", n.color)
            putFloat("alpha", n.alpha)
            putBoolean("right", n.right)
            putInt("maxVisible", n.maxVisible)
            putBoolean("panel", n.panel)
            putString("modes", Mode.listToJson(n.modes))
            putString("desktop", n.desktop.toJson())
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
