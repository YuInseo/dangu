package com.dangu.lumen

import org.json.JSONArray
import org.json.JSONObject

/** 페이지(classic.js)가 넘겨주는 디스코드 화면 상태. 서랍이 이걸로 그려진다. */
data class DGuild(val id: String, val name: String, val icon: String, val unread: Boolean, val mentions: Int) {
    val iconUrl: String? get() = if (icon.isEmpty()) null else "https://cdn.discordapp.com/icons/$id/$icon.png?size=96"
    /** 아이콘이 없는 서버는 옛날처럼 이름 머리글자로 */
    val initials: String
        get() = name.split(' ').filter { it.isNotBlank() }.take(3).joinToString("") { it.take(1) }.ifEmpty { "?" }
}

data class DChannel(
    val id: String,
    val name: String,
    val type: Int,
    val parent: String,
    val pos: Int,
    val unread: Boolean,
    val mentions: Int,
    val avatar: String,
    val user: String,
) {
    val isCategory get() = type == 4
    val isVoice get() = type == 2 || type == 13
    val avatarUrl: String?
        get() = if (user.isNotEmpty() && avatar.isNotEmpty()) "https://cdn.discordapp.com/avatars/$user/$avatar.png?size=64" else null
}

data class DMe(val id: String, val name: String, val avatar: String)

data class DiscordState(
    val ready: Boolean = false,
    /** 로그인한 나. 없으면 아직 로그인 전 — 그땐 웹 화면(로그인)을 보여 준다. */
    val me: DMe? = null,
    val title: String = "",
    val voice: Boolean = false,
    val guilds: List<DGuild> = emptyList(),
    val guild: String = "@me",
    val channel: String = "",
    val channels: List<DChannel> = emptyList(),
) {
    companion object {
        fun parse(json: String): DiscordState? = runCatching {
            val o = JSONObject(json)
            DiscordState(
                ready = o.optBoolean("ready"),
                me = o.optJSONObject("me")?.let { DMe(it.optString("id"), it.optString("name"), it.optString("avatar")) },
                title = o.optString("title"),
                voice = o.optBoolean("voice"),
                guilds = o.optJSONArray("guilds").objects().map {
                    DGuild(it.optString("id"), it.optString("name"), it.optString("icon"), it.optBoolean("unread"), it.optInt("mentions"))
                },
                guild = o.optString("guild", "@me"),
                channel = o.optString("channel"),
                channels = parseChannels(o.optJSONArray("channels")),
            )
        }.getOrNull()

        fun parseChannels(json: String): List<DChannel> = runCatching { parseChannels(JSONArray(json)) }.getOrDefault(emptyList())

        private fun parseChannels(a: JSONArray?): List<DChannel> = a.objects().map {
            DChannel(
                it.optString("id"), it.optString("name"), it.optInt("type"), it.optString("parent"), it.optInt("pos"),
                it.optBoolean("unread"), it.optInt("mentions"), it.optString("avatar"), it.optString("user"),
            )
        }

        private fun JSONArray?.objects(): List<JSONObject> =
            if (this == null) emptyList() else (0 until length()).mapNotNull { optJSONObject(it) }
    }
}

/**
 * 채널을 옛날 디스코드 순서로: 분류 없는 채널 먼저, 그다음 분류마다 (글 채널 → 음성 채널), 각각 위치순.
 * 결과는 (분류 또는 null, 그 안의 채널들).
 */
fun groupChannels(channels: List<DChannel>): List<Pair<DChannel?, List<DChannel>>> {
    val categories = channels.filter { it.isCategory }.sortedBy { it.pos }
    val ids = categories.map { it.id }.toSet()
    fun sorted(list: List<DChannel>) = list.sortedWith(compareBy({ it.isVoice }, { it.pos }, { it.name }))
    val out = ArrayList<Pair<DChannel?, List<DChannel>>>()
    val loose = channels.filter { !it.isCategory && it.parent !in ids }
    if (loose.isNotEmpty()) out += null to sorted(loose)
    for (cat in categories) {
        val inside = channels.filter { !it.isCategory && it.parent == cat.id }
        if (inside.isNotEmpty()) out += cat to sorted(inside)
    }
    return out
}
