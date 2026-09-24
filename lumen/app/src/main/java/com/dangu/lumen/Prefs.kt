package com.dangu.lumen

import android.content.Context
import androidx.core.content.edit
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow

/** 설정 전부. 바뀌면 [state]가 새 스냅샷을 내보내고, 화면이 그걸 보고 CSS를 다시 넣는다. */
class Prefs(context: Context) {
    private val sp = context.getSharedPreferences("lumen", Context.MODE_PRIVATE)

    data class Snapshot(
        val themeId: String,
        val hideSidebar: Boolean,
        val hideMembers: Boolean,
        val hideNitro: Boolean,
        val radius: Int,
        val textZoom: Int,
        val wideLayout: Boolean,
        val notifications: Boolean,
        val customCss: String,
        val bubbleX: Float,
        val bubbleY: Float,
        /** 0 데스크톱 크롬, 1 WebView 기본, 2 모바일 크롬 */
        val uaMode: Int,
        /** 테마·스크립트를 하나도 넣지 않는다 — 화면이 안 뜰 때 원인 가르기 */
        val safeMode: Boolean,
        /** 옛날 디스코드 서랍(서버 막대 + 채널 목록) */
        val classic: Boolean,
    )

    private fun read() = Snapshot(
        themeId = sp.getString("theme", "classic")!!,
        hideSidebar = sp.getBoolean("hideSidebar", false),
        hideMembers = sp.getBoolean("hideMembers", true),
        hideNitro = sp.getBoolean("hideNitro", true),
        radius = sp.getInt("radius", 14),
        textZoom = sp.getInt("textZoom", 100),
        wideLayout = sp.getBoolean("wideLayout", false),
        notifications = sp.getBoolean("notifications", true),
        customCss = sp.getString("customCss", "")!!,
        bubbleX = sp.getFloat("bubbleX", -1f),
        bubbleY = sp.getFloat("bubbleY", 0.35f),
        uaMode = sp.getInt("uaMode", 0),
        safeMode = sp.getBoolean("safeMode", false),
        classic = sp.getBoolean("classic", true),
    )

    private val _state = MutableStateFlow(read())
    val state: StateFlow<Snapshot> = _state.asStateFlow()
    val value: Snapshot get() = _state.value

    fun update(block: Snapshot.() -> Snapshot) {
        val n = value.block()
        sp.edit {
            putString("theme", n.themeId)
            putBoolean("hideSidebar", n.hideSidebar)
            putBoolean("hideMembers", n.hideMembers)
            putBoolean("hideNitro", n.hideNitro)
            putInt("radius", n.radius)
            putInt("textZoom", n.textZoom)
            putBoolean("wideLayout", n.wideLayout)
            putBoolean("notifications", n.notifications)
            putString("customCss", n.customCss)
            putFloat("bubbleX", n.bubbleX)
            putFloat("bubbleY", n.bubbleY)
            putInt("uaMode", n.uaMode)
            putBoolean("safeMode", n.safeMode)
            putBoolean("classic", n.classic)
        }
        _state.value = n
    }
}
