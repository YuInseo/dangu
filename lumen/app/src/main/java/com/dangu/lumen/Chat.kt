package com.dangu.lumen

import org.json.JSONArray
import org.json.JSONObject

/** 지금 채널의 메시지(classic.js의 messagesOf). 네이티브 채팅 화면이 이걸로 그려진다. */
data class DFile(val url: String, val name: String, val type: String, val w: Int, val h: Int) {
    val isImage get() = type.startsWith("image/")
}

data class DReaction(val emoji: String, val count: Int, val me: Boolean)
data class DReply(val name: String, val text: String)

data class DMessage(
    val id: String,
    val author: String,
    val name: String,
    val avatar: String,
    val color: String,
    val bot: Boolean,
    val time: Long,
    val edited: Boolean,
    val text: String,
    val type: Int,
    val state: String,
    val files: List<DFile>,
    val reply: DReply?,
    val reactions: List<DReaction>,
) {
    val avatarUrl: String?
        get() = if (avatar.isNotEmpty() && author.isNotEmpty()) "https://cdn.discordapp.com/avatars/$author/$avatar.png?size=80" else null
    val pending get() = state == "SENDING"
    val failed get() = state == "SEND_FAILED"

    /** 일반 메시지·답장·명령 응답이 아닌 것(입장 알림 등) */
    val isSystem get() = type != 0 && type != 19 && type != 20 && type != 23
}

data class ChatState(
    val channel: String,
    val title: String,
    val topic: String,
    val dm: Boolean,
    val hasMore: Boolean,
    val loading: Boolean,
    val messages: List<DMessage>,
) {
    companion object {
        fun parse(json: String): ChatState? = runCatching {
            val o = JSONObject(json)
            ChatState(
                channel = o.getString("channel"),
                title = o.optString("title"),
                topic = o.optString("topic"),
                dm = o.optBoolean("dm"),
                hasMore = o.optBoolean("hasMore"),
                loading = o.optBoolean("loading"),
                messages = o.optJSONArray("messages").objects().map { m ->
                    DMessage(
                        id = m.optString("id"),
                        author = m.optString("author"),
                        name = m.optString("name"),
                        avatar = m.optString("avatar"),
                        color = m.optString("color"),
                        bot = m.optBoolean("bot"),
                        time = m.optLong("time"),
                        edited = m.optBoolean("edited"),
                        text = m.optString("text"),
                        type = m.optInt("type"),
                        state = m.optString("state"),
                        files = m.optJSONArray("files").objects().map {
                            DFile(it.optString("url"), it.optString("name"), it.optString("type"), it.optInt("w"), it.optInt("h"))
                        },
                        reply = m.optJSONObject("reply")?.let { DReply(it.optString("name"), it.optString("text")) },
                        reactions = m.optJSONArray("reactions").objects().map {
                            DReaction(it.optString("emoji"), it.optInt("count"), it.optBoolean("me"))
                        },
                    )
                },
            )
        }.getOrNull()

        private fun JSONArray?.objects(): List<JSONObject> =
            if (this == null) emptyList() else (0 until length()).mapNotNull { optJSONObject(it) }
    }
}
