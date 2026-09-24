package com.dangu.lumen.ui

import android.content.Intent
import android.net.Uri
import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.Send
import androidx.compose.material.icons.filled.Add
import androidx.compose.material.icons.filled.Menu
import androidx.compose.material.icons.filled.MoreVert
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.derivedStateOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.LinkAnnotation
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.TextLinkStyles
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.font.FontStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.text.withLink
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import coil.compose.AsyncImage
import com.dangu.lumen.ChatState
import com.dangu.lumen.DMessage
import com.dangu.lumen.DiscordState
import com.dangu.lumen.MainActivity
import java.time.Instant
import java.time.LocalDate
import java.time.ZoneId
import java.time.format.DateTimeFormatter
import java.util.Locale

/** 옛날(2018~2021) 디스코드 모바일 채팅 화면의 색 */
private object C {
    val bg = Color(0xFF36393F)
    val bar = Color(0xFF36393F)
    val barLine = Color(0xFF202225)
    val input = Color(0xFF40444B)
    val text = Color(0xFFDCDDDE)
    val muted = Color(0xFF72767D)
    val name = Color(0xFFFFFFFF)
    val link = Color(0xFF00B0F4)
    val blurple = Color(0xFF7289DA)
    val reaction = Color(0xFF2F3136)
    val replyBar = Color(0xFF4F545C)
    val divider = Color(0xFF42454A)
    val red = Color(0xFFF04747)
}

private val zone: ZoneId get() = ZoneId.systemDefault()
private val timeFmt = DateTimeFormatter.ofPattern("a h:mm", Locale.KOREAN)
private val dayFmt = DateTimeFormatter.ofPattern("yyyy년 M월 d일", Locale.KOREAN)

private fun stamp(ms: Long): String {
    val t = Instant.ofEpochMilli(ms).atZone(zone)
    val today = LocalDate.now(zone)
    val d = t.toLocalDate()
    val prefix = when (d) {
        today -> "오늘"
        today.minusDays(1) -> "어제"
        else -> t.format(DateTimeFormatter.ofPattern("yyyy.MM.dd", Locale.KOREAN))
    }
    return "$prefix ${t.format(timeFmt)}"
}

/**
 * 옛날 디스코드 모바일의 채팅 화면. 메시지는 웹 클라이언트가 이미 받은 것을 그대로 그리고,
 * 보내기는 웹 클라이언트 자신의 보내기 함수로 한다.
 */
@Composable
fun NativeChat(
    activity: MainActivity,
    discord: DiscordState,
    chat: ChatState?,
    onMenu: () -> Unit,
    onSettings: () -> Unit,
) {
    val current = chat?.takeIf { it.channel == discord.channel }
    Column(Modifier.fillMaxSize().background(C.bg).statusBarsPadding().navigationBarsPadding().imePadding()) {
        TopBar(
            title = when {
                discord.channel.isEmpty() -> "Lumen"
                discord.voice -> "🔊 " + discord.title
                discord.guild == "@me" -> "@" + discord.title
                else -> "# " + discord.title
            },
            onMenu = onMenu,
            onWeb = { activity.showWeb.value = true },
            onReload = { activity.reload() },
            onSettings = onSettings,
        )

        Box(Modifier.weight(1f).fillMaxWidth()) {
            when {
                discord.channel.isEmpty() -> Empty("왼쪽 서랍에서 서버나 친구를 골라 주세요", "서랍 열기", onMenu)
                discord.voice -> Empty("음성 채널이에요. 참가·화면 보기는 웹 화면에서 할 수 있어요.", "웹 화면으로 참가", { activity.showWeb.value = true })
                current == null -> Empty("메시지를 불러오는 중…", null, null)
                else -> Messages(activity, current)
            }
        }

        if (discord.channel.isNotEmpty() && !discord.voice) {
            InputBar(
                placeholder = if (discord.guild == "@me") "@${discord.title}에게 메시지 보내기" else "#${discord.title}에 메시지 보내기",
                onSend = { activity.sendMessage(discord.channel, it) },
                onAttach = { activity.showWeb.value = true },
            )
        }
    }
}

@Composable
private fun TopBar(title: String, onMenu: () -> Unit, onWeb: () -> Unit, onReload: () -> Unit, onSettings: () -> Unit) {
    Column {
        Row(Modifier.fillMaxWidth().height(56.dp).background(C.bar), verticalAlignment = Alignment.CenterVertically) {
            IconButton(onClick = onMenu) { Icon(Icons.Filled.Menu, "서랍", tint = C.text) }
            Text(
                title, color = C.name, fontWeight = FontWeight.Bold, fontSize = 17.sp,
                maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.weight(1f),
            )
            var menu by remember { mutableStateOf(false) }
            Box {
                IconButton(onClick = { menu = true }) { Icon(Icons.Filled.MoreVert, "더 보기", tint = C.text) }
                DropdownMenu(expanded = menu, onDismissRequest = { menu = false }) {
                    DropdownMenuItem(text = { Text("웹 화면으로 보기") }, onClick = { menu = false; onWeb() })
                    DropdownMenuItem(text = { Text("새로고침") }, onClick = { menu = false; onReload() })
                    DropdownMenuItem(text = { Text("Lumen 설정") }, onClick = { menu = false; onSettings() })
                }
            }
        }
        Box(Modifier.fillMaxWidth().height(1.dp).background(C.barLine))
    }
}

@Composable
private fun Empty(text: String, action: String?, onAction: (() -> Unit)?) {
    Column(
        Modifier.fillMaxSize().padding(32.dp),
        verticalArrangement = Arrangement.Center,
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        Text(text, color = C.muted, fontSize = 15.sp)
        if (action != null && onAction != null) {
            Spacer(Modifier.height(16.dp))
            Box(
                Modifier.clip(RoundedCornerShape(4.dp)).background(C.blurple).clickable(onClick = onAction)
                    .padding(horizontal = 20.dp, vertical = 10.dp)
            ) { Text(action, color = Color.White, fontWeight = FontWeight.Medium) }
        }
    }
}

/** 같은 사람이 7분 안에 이어 쓴 메시지는 머리(아바타·이름) 없이 붙인다 — 옛날 그대로. */
private fun startsGroup(prev: DMessage?, m: DMessage): Boolean {
    if (prev == null || m.reply != null || m.isSystem || prev.isSystem) return true
    if (prev.author != m.author) return true
    if (m.time - prev.time > 7 * 60 * 1000) return true
    return Instant.ofEpochMilli(prev.time).atZone(zone).toLocalDate() != Instant.ofEpochMilli(m.time).atZone(zone).toLocalDate()
}

@Composable
private fun Messages(activity: MainActivity, chat: ChatState) {
    val list = chat.messages
    val state = rememberLazyListState()
    // 새 메시지가 오면 맨 아래에 있을 때만 따라 내려간다.
    LaunchedEffect(chat.channel) { state.scrollToItem(0) }
    LaunchedEffect(list.lastOrNull()?.id) {
        if (state.firstVisibleItemIndex <= 1) state.animateScrollToItem(0)
    }
    // 위 끝에 닿으면 예전 메시지
    val nearTop by remember { derivedStateOf { state.layoutInfo.visibleItemsInfo.lastOrNull()?.index ?: 0 } }
    LaunchedEffect(nearTop, list.size) {
        if (chat.hasMore && !chat.loading && list.isNotEmpty() && nearTop >= list.size - 3) activity.loadOlder(chat.channel)
    }

    LazyColumn(
        state = state,
        reverseLayout = true,
        modifier = Modifier.fillMaxSize(),
        contentPadding = PaddingValues(vertical = 12.dp),
    ) {
        items(list.size, key = { list[list.size - 1 - it].id }) { i ->
            val idx = list.size - 1 - i
            val m = list[idx]
            val prev = list.getOrNull(idx - 1)
            Column {
                val day = Instant.ofEpochMilli(m.time).atZone(zone).toLocalDate()
                if (prev == null || Instant.ofEpochMilli(prev.time).atZone(zone).toLocalDate() != day) DayDivider(day)
                MessageRow(m, startsGroup(prev, m))
            }
        }
    }
}

@Composable
private fun DayDivider(day: LocalDate) {
    Row(Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 12.dp), verticalAlignment = Alignment.CenterVertically) {
        Box(Modifier.weight(1f).height(1.dp).background(C.divider))
        Text(day.format(dayFmt), color = C.muted, fontSize = 12.sp, fontWeight = FontWeight.SemiBold, modifier = Modifier.padding(horizontal = 8.dp))
        Box(Modifier.weight(1f).height(1.dp).background(C.divider))
    }
}

private fun parseColor(hex: String): Color? =
    runCatching { Color(android.graphics.Color.parseColor(hex)) }.getOrNull()?.takeIf { hex.isNotEmpty() }

@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun MessageRow(m: DMessage, head: Boolean) {
    if (m.isSystem) {
        Text(
            "→  " + (m.text.ifEmpty { "${m.name}님의 시스템 메시지" }) + "   " + stamp(m.time),
            color = C.muted, fontSize = 13.sp, fontStyle = FontStyle.Italic,
            modifier = Modifier.padding(start = 72.dp, end = 16.dp, top = 6.dp, bottom = 2.dp),
        )
        return
    }
    Column(Modifier.fillMaxWidth().padding(top = if (head) 14.dp else 2.dp)) {
        if (m.reply != null) {
            Row(Modifier.padding(start = 36.dp, end = 16.dp, bottom = 2.dp), verticalAlignment = Alignment.CenterVertically) {
                Box(Modifier.width(28.dp).height(10.dp).padding(top = 5.dp).background(C.replyBar))
                Spacer(Modifier.width(6.dp))
                Text(
                    "@${m.reply.name}  ${m.reply.text}", color = C.muted, fontSize = 13.sp,
                    maxLines = 1, overflow = TextOverflow.Ellipsis,
                )
            }
        }
        Row(Modifier.fillMaxWidth().padding(start = 16.dp, end = 16.dp)) {
            if (head) {
                Box(Modifier.size(40.dp).clip(CircleShape).background(C.blurple), contentAlignment = Alignment.Center) {
                    val url = m.avatarUrl
                    if (url != null) AsyncImage(url, null, contentScale = ContentScale.Crop, modifier = Modifier.fillMaxSize())
                    else Text(m.name.take(1), color = Color.White, fontSize = 16.sp, fontWeight = FontWeight.Bold)
                }
            } else {
                Spacer(Modifier.width(40.dp))
            }
            Spacer(Modifier.width(16.dp))
            Column(Modifier.weight(1f)) {
                if (head) {
                    Row(verticalAlignment = Alignment.CenterVertically) {
                        Text(
                            m.name, color = parseColor(m.color) ?: C.name, fontWeight = FontWeight.SemiBold, fontSize = 15.sp,
                            maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.weight(1f, fill = false),
                        )
                        if (m.bot) {
                            Spacer(Modifier.width(6.dp))
                            Text(
                                "BOT", color = Color.White, fontSize = 10.sp, fontWeight = FontWeight.Bold,
                                modifier = Modifier.clip(RoundedCornerShape(3.dp)).background(C.blurple).padding(horizontal = 4.dp, vertical = 1.dp),
                            )
                        }
                        Spacer(Modifier.width(8.dp))
                        Text(stamp(m.time), color = C.muted, fontSize = 12.sp)
                    }
                    Spacer(Modifier.height(2.dp))
                }
                if (m.text.isNotEmpty()) {
                    Text(
                        linkify(m.text),
                        style = TextStyle(
                            color = when {
                                m.failed -> C.red
                                m.pending -> C.muted
                                else -> C.text
                            },
                            fontSize = 15.sp, lineHeight = 21.sp,
                        ),
                    )
                }
                if (m.edited) Text("(수정됨)", color = C.muted, fontSize = 10.sp)
                m.files.forEach { f -> Attachment(f) }
                if (m.reactions.isNotEmpty()) {
                    FlowRow(
                        Modifier.padding(top = 4.dp),
                        horizontalArrangement = Arrangement.spacedBy(4.dp),
                        verticalArrangement = Arrangement.spacedBy(4.dp),
                    ) {
                        m.reactions.forEach { r ->
                            Row(
                                Modifier
                                    .clip(RoundedCornerShape(8.dp))
                                    .background(if (r.me) C.blurple.copy(alpha = 0.3f) else C.reaction)
                                    .then(if (r.me) Modifier.border(BorderStroke(1.dp, C.blurple), RoundedCornerShape(8.dp)) else Modifier)
                                    .padding(horizontal = 6.dp, vertical = 2.dp),
                                verticalAlignment = Alignment.CenterVertically,
                            ) {
                                Text(r.emoji.ifEmpty { "?" }, fontSize = 14.sp)
                                Spacer(Modifier.width(4.dp))
                                Text("${r.count}", color = C.text, fontSize = 12.sp, fontWeight = FontWeight.SemiBold)
                            }
                        }
                    }
                }
            }
        }
    }
}

@Composable
private fun Attachment(f: com.dangu.lumen.DFile) {
    val context = LocalContext.current
    val open = { runCatching { context.startActivity(Intent(Intent.ACTION_VIEW, Uri.parse(f.url)).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)) } }
    if (f.isImage && f.url.isNotEmpty()) {
        val ratio = if (f.w > 0 && f.h > 0) (f.w.toFloat() / f.h).coerceIn(0.4f, 3f) else 1.5f
        AsyncImage(
            model = f.url,
            contentDescription = f.name,
            contentScale = ContentScale.Crop,
            modifier = Modifier
                .padding(top = 6.dp)
                .widthIn(max = 280.dp)
                .fillMaxWidth()
                .aspectRatio(ratio)
                .heightIn(max = 320.dp)
                .clip(RoundedCornerShape(4.dp))
                .background(C.reaction)
                .clickable { open() },
        )
    } else if (f.url.isNotEmpty()) {
        Row(
            Modifier
                .padding(top = 6.dp)
                .clip(RoundedCornerShape(4.dp))
                .background(C.reaction)
                .clickable { open() }
                .padding(12.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text("📄", fontSize = 22.sp)
            Spacer(Modifier.width(8.dp))
            Text(f.name.ifEmpty { "파일" }, color = C.link, fontSize = 14.sp, maxLines = 1, overflow = TextOverflow.Ellipsis)
        }
    }
}

private val urlRegex = Regex("""https?://[^\s<>]+""")

/** 주소는 누를 수 있게, 멘션(@…)은 옛날처럼 파란 배경 없이 강조색으로. */
private fun linkify(text: String): AnnotatedString = buildAnnotatedString {
    var last = 0
    for (m in urlRegex.findAll(text)) {
        appendMentions(text.substring(last, m.range.first))
        withLink(LinkAnnotation.Url(m.value, TextLinkStyles(SpanStyle(color = C.link)))) { append(m.value) }
        last = m.range.last + 1
    }
    appendMentions(text.substring(last))
}

private val mentionRegex = Regex("""@[^\s@#]+|#[^\s@#]+""")

private fun AnnotatedString.Builder.appendMentions(s: String) {
    var last = 0
    for (m in mentionRegex.findAll(s)) {
        append(s.substring(last, m.range.first))
        pushStyle(SpanStyle(color = Color(0xFFC9CDFB), background = C.blurple.copy(alpha = 0.3f), fontWeight = FontWeight.Medium))
        append(m.value)
        pop()
        last = m.range.last + 1
    }
    append(s.substring(last))
}

@Composable
private fun InputBar(placeholder: String, onSend: (String) -> Unit, onAttach: () -> Unit) {
    var text by remember { mutableStateOf("") }
    Row(
        Modifier.fillMaxWidth().background(C.bg).padding(horizontal = 8.dp, vertical = 8.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Box(
            Modifier.size(40.dp).clip(CircleShape).background(C.input).clickable(onClick = onAttach),
            contentAlignment = Alignment.Center,
        ) { Icon(Icons.Filled.Add, "파일 올리기(웹 화면)", tint = C.text) }
        Spacer(Modifier.width(8.dp))
        Box(
            Modifier.weight(1f).clip(RoundedCornerShape(20.dp)).background(C.input).padding(horizontal = 16.dp, vertical = 11.dp),
        ) {
            if (text.isEmpty()) Text(placeholder, color = C.muted, fontSize = 15.sp, maxLines = 1, overflow = TextOverflow.Ellipsis)
            BasicTextField(
                value = text,
                onValueChange = { text = it },
                textStyle = TextStyle(color = C.text, fontSize = 15.sp),
                cursorBrush = SolidColor(C.text),
                maxLines = 5,
                modifier = Modifier.fillMaxWidth(),
            )
        }
        if (text.isNotBlank()) {
            Spacer(Modifier.width(8.dp))
            Box(
                Modifier.size(40.dp).clip(CircleShape).background(C.blurple).clickable {
                    onSend(text.trim())
                    text = ""
                },
                contentAlignment = Alignment.Center,
            ) { Icon(Icons.AutoMirrored.Filled.Send, "보내기", tint = Color.White, modifier = Modifier.size(20.dp)) }
        }
    }
}
