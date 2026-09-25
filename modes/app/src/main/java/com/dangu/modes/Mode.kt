package com.dangu.modes

import org.json.JSONArray
import org.json.JSONObject

/** 모드 종류. 팝업 목록 한 줄 = Mode 하나. */
enum class ModeType(val label: String, val emoji: String) {
    HOME("기본 바탕화면", "🏠"),
    SECRET("비밀 바탕화면", "🔒"),
    WORK("업무 프로필", "💼"),
    LAUNCH("앱 열기", "📱"),
    PIN("앱 고정", "📌"),
}

data class Mode(
    val id: String,
    val type: ModeType,
    val label: String,
    /** LAUNCH·PIN이 여는 앱 */
    val packageName: String = "",
) {
    val needsApp get() = type == ModeType.LAUNCH || type == ModeType.PIN

    fun toJson(): JSONObject = JSONObject()
        .put("id", id).put("type", type.name).put("label", label).put("package", packageName)

    companion object {
        fun fromJson(o: JSONObject): Mode? = runCatching {
            Mode(o.getString("id"), ModeType.valueOf(o.getString("type")), o.optString("label"), o.optString("package"))
        }.getOrNull()

        fun listToJson(list: List<Mode>): String = JSONArray().apply { list.forEach { put(it.toJson()) } }.toString()

        fun listFromJson(json: String?): List<Mode>? = runCatching {
            val a = JSONArray(json ?: return null)
            (0 until a.length()).mapNotNull { fromJson(a.getJSONObject(it)) }
        }.getOrNull()

        fun newId() = java.util.UUID.randomUUID().toString().take(8)

        val defaults = listOf(
            Mode("home", ModeType.HOME, "기본 바탕화면"),
            Mode("secret", ModeType.SECRET, "비밀 바탕화면"),
            Mode("work", ModeType.WORK, "업무 프로필"),
            Mode("pin-settings", ModeType.PIN, "설정 앱 고정", "com.android.settings"),
        )
    }
}
