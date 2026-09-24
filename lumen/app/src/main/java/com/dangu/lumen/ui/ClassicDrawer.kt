package com.dangu.lumen.ui

import androidx.activity.compose.BackHandler
import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.tween
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.detectHorizontalDragGestures
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.systemGestureExclusion
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.Settings
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateListOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import coil.compose.AsyncImage
import com.dangu.lumen.DChannel
import com.dangu.lumen.DGuild
import com.dangu.lumen.MainActivity
import com.dangu.lumen.groupChannels
import kotlinx.coroutines.launch
import kotlin.math.roundToInt

/** 옛날(2018~2021) 디스코드 모바일의 색 */
private object Old {
    val rail = Color(0xFF202225)
    val panel = Color(0xFF2F3136)
    val header = Color(0xFF292B2F)
    val selected = Color(0xFF393C43)
    val hover = Color(0xFF34373C)
    val text = Color(0xFFDCDDDE)
    val muted = Color(0xFF8E9297)
    val white = Color(0xFFFFFFFF)
    val blurple = Color(0xFF7289DA)
    val red = Color(0xFFF04747)
    val serverBg = Color(0xFF36393F)
}

/**
 * 옛날 디스코드 모바일의 왼쪽 서랍 — 서버 막대와 채널 목록.
 * 왼쪽 가장자리에서 오른쪽으로 쓸거나 떠 있는 단추를 누르면 열린다.
 */
@Composable
fun ClassicDrawer(activity: MainActivity, open: Boolean, onOpenChange: (Boolean) -> Unit, onSettings: () -> Unit) {
    val state by activity.discord
    val density = LocalDensity.current
    val scope = rememberCoroutineScope()

    BoxWithConstraints(Modifier.fillMaxSize()) {
        val widthPx = with(density) { minOf(maxWidth * 0.88f, 380.dp).toPx() }
        val offset = remember { Animatable(-widthPx) }
        LaunchedEffect(open, widthPx) { offset.animateTo(if (open) 0f else -widthPx, tween(220)) }
        val progress = ((offset.value + widthPx) / widthPx).coerceIn(0f, 1f)

        fun settle() {
            val target = offset.value > -widthPx / 2
            scope.launch { offset.animateTo(if (target) 0f else -widthPx, tween(180)) }
            onOpenChange(target)
        }

        // 왼쪽 가장자리 — 여기서 오른쪽으로 쓸면 서랍이 따라 나온다.
        if (!open) {
            Box(
                Modifier
                    .align(Alignment.CenterStart)
                    .width(18.dp)
                    .fillMaxHeight(0.6f)
                    // 제스처 내비게이션의 "뒤로"와 겹치지 않게 이 띠는 앱이 받는다.
                    .systemGestureExclusion()
                    .pointerInput(widthPx) {
                        detectHorizontalDragGestures(onDragEnd = { settle() }, onDragCancel = { settle() }) { change, dx ->
                            change.consume()
                            scope.launch { offset.snapTo((offset.value + dx).coerceIn(-widthPx, 0f)) }
                        }
                    },
            )
        }

        if (progress > 0f) {
            // 뒤 어둡게, 누르면 닫힘
            Box(
                Modifier
                    .fillMaxSize()
                    .background(Color.Black.copy(alpha = 0.55f * progress))
                    .pointerInput(Unit) { detectTapGestures { onOpenChange(false) } }
                    .pointerInput(widthPx) {
                        detectHorizontalDragGestures(onDragEnd = { settle() }, onDragCancel = { settle() }) { change, dx ->
                            change.consume()
                            scope.launch { offset.snapTo((offset.value + dx).coerceIn(-widthPx, 0f)) }
                        }
                    },
            )
            BackHandler(enabled = open) { onOpenChange(false) }

            Row(
                Modifier
                    .offset { IntOffset(offset.value.roundToInt(), 0) }
                    .width(with(density) { widthPx.toDp() })
                    .fillMaxHeight()
                    .background(Old.rail)
                    .statusBarsPadding()
                    .navigationBarsPadding(),
            ) {
                var shownGuild by remember { mutableStateOf(state.guild) }
                LaunchedEffect(state.guild) { shownGuild = state.guild }

                GuildRail(
                    guilds = state.guilds,
                    selected = shownGuild,
                    onSelect = { shownGuild = it },
                )
                ChannelPanel(
                    activity = activity,
                    guildId = shownGuild,
                    guild = state.guilds.find { it.id == shownGuild },
                    current = state.channel,
                    liveChannels = if (shownGuild == state.guild) state.channels else null,
                    ready = state.ready,
                    onOpen = { cid ->
                        activity.openChannel(shownGuild, cid)
                        onOpenChange(false)
                    },
                    onSettings = onSettings,
                    modifier = Modifier.weight(1f),
                )
            }
        }
    }
}

@Composable
private fun GuildRail(guilds: List<DGuild>, selected: String, onSelect: (String) -> Unit) {
    LazyColumn(
        Modifier.width(72.dp).fillMaxHeight(),
        contentPadding = PaddingValues(vertical = 10.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        item {
            GuildIcon(
                selected = selected == "@me",
                unread = false,
                mentions = 0,
                onClick = { onSelect("@me") },
                background = if (selected == "@me") Old.blurple else Old.serverBg,
            ) {
                // 옛 DM 단추: 말풍선
                Text("💬", fontSize = 20.sp)
            }
        }
        item {
            Box(Modifier.width(32.dp).height(2.dp).clip(RoundedCornerShape(1.dp)).background(Old.serverBg))
        }
        items(guilds, key = { it.id }) { g ->
            GuildIcon(
                selected = g.id == selected,
                unread = g.unread,
                mentions = g.mentions,
                onClick = { onSelect(g.id) },
                background = if (g.id == selected) Old.blurple else Old.serverBg,
            ) {
                val url = g.iconUrl
                if (url != null) {
                    AsyncImage(model = url, contentDescription = g.name, contentScale = ContentScale.Crop, modifier = Modifier.fillMaxSize())
                } else {
                    Text(g.initials, color = Old.text, fontSize = 14.sp, fontWeight = FontWeight.Medium, maxLines = 1)
                }
            }
        }
    }
}

/** 동그란 서버 아이콘. 고르면 모서리가 둥근 네모가 되고 왼쪽에 흰 알약이 붙는다 — 옛날 그대로. */
@Composable
private fun GuildIcon(
    selected: Boolean,
    unread: Boolean,
    mentions: Int,
    onClick: () -> Unit,
    background: Color,
    content: @Composable () -> Unit,
) {
    Box(Modifier.width(72.dp).height(48.dp)) {
        if (selected || unread) {
            Box(
                Modifier
                    .align(Alignment.CenterStart)
                    .width(4.dp)
                    .height(if (selected) 40.dp else 8.dp)
                    .clip(RoundedCornerShape(topEnd = 4.dp, bottomEnd = 4.dp))
                    .background(Old.white)
            )
        }
        Box(
            Modifier
                .align(Alignment.Center)
                .size(48.dp)
                .clip(if (selected) RoundedCornerShape(16.dp) else CircleShape)
                .background(background)
                .clickable(onClick = onClick),
            contentAlignment = Alignment.Center,
        ) { content() }
        if (mentions > 0) {
            Box(
                Modifier
                    .align(Alignment.BottomEnd)
                    .padding(end = 8.dp)
                    .size(20.dp)
                    .clip(CircleShape)
                    .background(Old.rail)
                    .padding(2.dp)
                    .clip(CircleShape)
                    .background(Old.red),
                contentAlignment = Alignment.Center,
            ) {
                Text(if (mentions > 99) "99" else "$mentions", color = Old.white, fontSize = 10.sp, fontWeight = FontWeight.Bold)
            }
        }
    }
}

@Composable
private fun ChannelPanel(
    activity: MainActivity,
    guildId: String,
    guild: DGuild?,
    current: String,
    liveChannels: List<DChannel>?,
    ready: Boolean,
    onOpen: (String) -> Unit,
    onSettings: () -> Unit,
    modifier: Modifier = Modifier,
) {
    // 지금 디스코드에서 열린 서버면 실시간 목록, 아니면 한 번 읽어 온다.
    var fetched by remember(guildId) { mutableStateOf<List<DChannel>?>(null) }
    LaunchedEffect(guildId, liveChannels == null) {
        if (liveChannels == null) activity.loadChannels(guildId) { fetched = it }
    }
    val channels = liveChannels ?: fetched
    val collapsed = remember(guildId) { mutableStateListOf<String>() }
    val isDm = guildId == "@me"

    Column(
        modifier
            .fillMaxHeight()
            .clip(RoundedCornerShape(topStart = 8.dp))
            .background(Old.panel),
    ) {
        Box(
            Modifier.fillMaxWidth().height(48.dp).background(Old.header).padding(horizontal = 16.dp),
            contentAlignment = Alignment.CenterStart,
        ) {
            Text(
                if (isDm) "다이렉트 메시지" else guild?.name ?: "",
                color = Old.white, fontWeight = FontWeight.Bold, fontSize = 16.sp,
                maxLines = 1, overflow = TextOverflow.Ellipsis,
            )
        }

        when {
            !ready -> Hint("디스코드가 준비되는 중… (로그인 후에 목록이 나타납니다)")
            channels == null -> Hint("불러오는 중…")
            channels.isEmpty() -> Hint(if (isDm) "대화가 없습니다" else "볼 수 있는 채널이 없습니다")
            else -> LazyColumn(Modifier.weight(1f), contentPadding = PaddingValues(vertical = 8.dp)) {
                if (isDm) {
                    items(channels, key = { it.id }) { c -> DmRow(c, c.id == current) { onOpen(c.id) } }
                } else {
                    groupChannels(channels).forEach { (cat, list) ->
                        if (cat != null) {
                            item(key = "cat-" + cat.id) {
                                val closed = cat.id in collapsed
                                Text(
                                    (if (closed) "›  " else "⌄  ") + cat.name.uppercase(),
                                    color = Old.muted, fontSize = 12.sp, fontWeight = FontWeight.Bold,
                                    maxLines = 1, overflow = TextOverflow.Ellipsis,
                                    modifier = Modifier
                                        .fillMaxWidth()
                                        .clickable { if (closed) collapsed.remove(cat.id) else collapsed.add(cat.id) }
                                        .padding(start = 12.dp, end = 12.dp, top = 16.dp, bottom = 4.dp),
                                )
                            }
                        }
                        if (cat == null || cat.id !in collapsed) {
                            items(list, key = { it.id }) { c -> ChannelRow(c, c.id == current) { onOpen(c.id) } }
                        } else {
                            // 접힌 분류에서도 안 읽은 채널과 지금 채널은 보인다(옛날 그대로).
                            items(list.filter { it.unread || it.id == current }, key = { it.id }) { c ->
                                ChannelRow(c, c.id == current) { onOpen(c.id) }
                            }
                        }
                    }
                }
            }
        }

        Row(
            Modifier
                .fillMaxWidth()
                .background(Old.header)
                .clickable(onClick = onSettings)
                .padding(horizontal = 16.dp, vertical = 12.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Icon(Icons.Outlined.Settings, null, tint = Old.muted, modifier = Modifier.size(20.dp))
            Spacer(Modifier.width(10.dp))
            Text("Lumen 설정", color = Old.muted, fontSize = 14.sp)
        }
    }
}

@Composable
private fun Hint(text: String) {
    Text(text, color = Old.muted, fontSize = 13.sp, modifier = Modifier.padding(16.dp))
}

@Composable
private fun ChannelRow(c: DChannel, selected: Boolean, onClick: () -> Unit) {
    val bright = selected || c.unread
    Box(Modifier.fillMaxWidth().padding(horizontal = 8.dp, vertical = 1.dp)) {
        if (c.unread && !selected) {
            Box(
                Modifier
                    .align(Alignment.CenterStart)
                    .offset(x = (-8).dp)
                    .width(4.dp)
                    .height(8.dp)
                    .clip(RoundedCornerShape(topEnd = 4.dp, bottomEnd = 4.dp))
                    .background(Old.white)
            )
        }
        Row(
            Modifier
                .fillMaxWidth()
                .clip(RoundedCornerShape(4.dp))
                .background(if (selected) Old.selected else Color.Transparent)
                .clickable(onClick = onClick)
                .padding(horizontal = 8.dp, vertical = 7.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text(
                if (c.isVoice) "🔊" else "#",
                color = Old.muted,
                fontSize = if (c.isVoice) 14.sp else 20.sp,
                modifier = Modifier.width(24.dp),
            )
            Text(
                c.name,
                color = if (bright) Old.white else Old.muted,
                fontWeight = if (c.unread) FontWeight.SemiBold else FontWeight.Medium,
                fontSize = 15.sp,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
                modifier = Modifier.weight(1f),
            )
            if (c.mentions > 0) {
                Box(Modifier.clip(RoundedCornerShape(8.dp)).background(Old.red).padding(horizontal = 6.dp, vertical = 1.dp)) {
                    Text("${c.mentions}", color = Old.white, fontSize = 11.sp, fontWeight = FontWeight.Bold)
                }
            }
        }
    }
}

@Composable
private fun DmRow(c: DChannel, selected: Boolean, onClick: () -> Unit) {
    Row(
        Modifier
            .fillMaxWidth()
            .padding(horizontal = 8.dp, vertical = 1.dp)
            .clip(RoundedCornerShape(4.dp))
            .background(if (selected) Old.selected else Color.Transparent)
            .clickable(onClick = onClick)
            .padding(horizontal = 8.dp, vertical = 6.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Box(Modifier.size(32.dp).clip(CircleShape).background(Old.blurple), contentAlignment = Alignment.Center) {
            val url = c.avatarUrl
            if (url != null) {
                AsyncImage(model = url, contentDescription = null, contentScale = ContentScale.Crop, modifier = Modifier.fillMaxSize())
            } else {
                Text(c.name.take(1), color = Old.white, fontSize = 14.sp)
            }
        }
        Spacer(Modifier.width(12.dp))
        Text(
            c.name,
            color = if (selected || c.unread) Old.white else Old.muted,
            fontWeight = if (c.unread) FontWeight.SemiBold else FontWeight.Medium,
            fontSize = 15.sp, maxLines = 1, overflow = TextOverflow.Ellipsis,
            modifier = Modifier.weight(1f),
        )
        if (c.mentions > 0) {
            Box(Modifier.clip(RoundedCornerShape(8.dp)).background(Old.red).padding(horizontal = 6.dp, vertical = 1.dp)) {
                Text("${c.mentions}", color = Old.white, fontSize = 11.sp, fontWeight = FontWeight.Bold)
            }
        }
    }
}
