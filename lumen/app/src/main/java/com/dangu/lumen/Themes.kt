package com.dangu.lumen

/**
 * Lumen의 모양.
 *
 * 디스코드 웹 클라이언트는 색을 전부 CSS 변수로 칠한다. 그 변수들을 덮어쓰면 기능은 하나도
 * 건드리지 않고 겉모습만 바뀐다. 디스코드가 이름을 바꿔 온 탓에 옛 이름과 새 이름을 둘 다 적는다 —
 * 모르는 변수는 그냥 무시되므로 해가 없다.
 */
data class LumenTheme(
    val id: String,
    val name: String,
    /** 가장 어두운 바탕(서버 막대) */
    val base: Long,
    /** 채널 목록 */
    val side: Long,
    /** 채팅 바탕 */
    val chat: Long,
    /** 입력창·카드·팝업 */
    val surface: Long,
    val text: Long,
    val muted: Long,
    val accent: Long,
    val light: Boolean = false,
)

object Themes {
    val all = listOf(
        // 2017~2021 디스코드 — 블러플 #7289DA, 회색 판들. 비주얼 리프레시 이전의 그 색.
        LumenTheme("classic", "클래식", 0xFF202225, 0xFF2F3136, 0xFF36393F, 0xFF40444B, 0xFFDCDDDE, 0xFF8E9297, 0xFF7289DA),
        LumenTheme("midnight", "미드나잇", 0xFF000000, 0xFF07080C, 0xFF0B0D12, 0xFF161922, 0xFFE8EAF2, 0xFF8A90A6, 0xFF8B7CFF),
        LumenTheme("ocean", "오션", 0xFF06121C, 0xFF0A1926, 0xFF0E2030, 0xFF163047, 0xFFE3F1FF, 0xFF86A6C4, 0xFF3DB2FF),
        LumenTheme("forest", "포레스트", 0xFF0A120D, 0xFF0F1A13, 0xFF142119, 0xFF1E3025, 0xFFE4F3E8, 0xFF8FB39A, 0xFF4ED18A),
        LumenTheme("rose", "로즈", 0xFF140A0F, 0xFF1C0F16, 0xFF24131C, 0xFF341C29, 0xFFFBE7EF, 0xFFC298AC, 0xFFFF6FA5),
        LumenTheme("mocha", "모카", 0xFF15110E, 0xFF1C1713, 0xFF231D18, 0xFF322a23, 0xFFF3E9DF, 0xFFB5A290, 0xFFE0A26B),
        LumenTheme("paper", "페이퍼", 0xFFE9E5DC, 0xFFF1EEE7, 0xFFFAF8F3, 0xFFFFFFFF, 0xFF26221C, 0xFF7C756A, 0xFF5B5BD6, light = true),
        LumenTheme("original", "원래대로", 0, 0, 0, 0, 0, 0, 0),
    )

    fun byId(id: String?) = all.find { it.id == id } ?: all.first()

    private fun hex(c: Long) = "#%06X".format(c and 0xFFFFFF)
    private fun rgba(c: Long, a: Double) =
        "rgba(${(c shr 16) and 0xFF}, ${(c shr 8) and 0xFF}, ${c and 0xFF}, $a)"

    fun css(theme: LumenTheme, prefs: Prefs.Snapshot): String = buildString {
        if (theme.id != "original") {
            val base = hex(theme.base)
            val side = hex(theme.side)
            val chat = hex(theme.chat)
            val surface = hex(theme.surface)
            val text = hex(theme.text)
            val muted = hex(theme.muted)
            val accent = hex(theme.accent)
            append(":root, .theme-dark, .theme-light, .theme-darker, .theme-midnight, .visual-refresh {\n")
            // 옛 이름
            append("--background-primary: $chat !important;\n")
            append("--background-secondary: $side !important;\n")
            append("--background-secondary-alt: $base !important;\n")
            append("--background-tertiary: $base !important;\n")
            append("--background-floating: $surface !important;\n")
            append("--background-nested-floating: $surface !important;\n")
            append("--background-accent: $accent !important;\n")
            append("--background-message-hover: ${rgba(theme.surface, 0.5)} !important;\n")
            append("--background-modifier-hover: ${rgba(theme.text, 0.06)} !important;\n")
            append("--background-modifier-active: ${rgba(theme.text, 0.1)} !important;\n")
            append("--background-modifier-selected: ${rgba(theme.accent, 0.22)} !important;\n")
            append("--background-modifier-accent: ${rgba(theme.text, 0.08)} !important;\n")
            append("--channeltextarea-background: $surface !important;\n")
            append("--modal-background: $side !important;\n")
            append("--modal-footer-background: $base !important;\n")
            append("--input-background: $surface !important;\n")
            append("--deprecated-card-bg: $surface !important;\n")
            append("--home-background: $chat !important;\n")
            append("--bg-overlay-chat: $chat !important;\n")
            append("--text-normal: $text !important;\n")
            append("--text-muted: $muted !important;\n")
            append("--header-primary: $text !important;\n")
            append("--header-secondary: $muted !important;\n")
            append("--channels-default: $muted !important;\n")
            append("--interactive-normal: $muted !important;\n")
            append("--interactive-hover: $text !important;\n")
            append("--interactive-active: $text !important;\n")
            append("--text-link: $accent !important;\n")
            append("--brand-experiment: $accent !important;\n")
            append("--brand-500: $accent !important;\n")
            append("--brand-560: $accent !important;\n")
            append("--brand-600: $accent !important;\n")
            append("--button-filled-brand-background: $accent !important;\n")
            append("--button-filled-brand-background-hover: $accent !important;\n")
            append("--scrollbar-auto-thumb: ${rgba(theme.text, 0.15)} !important;\n")
            append("--scrollbar-thin-thumb: ${rgba(theme.text, 0.15)} !important;\n")
            // 새 이름 (2024~ 비주얼 리프레시)
            append("--background-base-lowest: $base !important;\n")
            append("--background-base-lower: $side !important;\n")
            append("--background-base-low: $chat !important;\n")
            append("--background-surface-high: $surface !important;\n")
            append("--background-surface-higher: $surface !important;\n")
            append("--background-surface-highest: $surface !important;\n")
            append("--bg-base-primary: $chat !important;\n")
            append("--bg-base-secondary: $side !important;\n")
            append("--bg-base-tertiary: $base !important;\n")
            append("--bg-surface-overlay: $surface !important;\n")
            append("--bg-surface-raised: $surface !important;\n")
            append("--chat-background-default: $chat !important;\n")
            append("--text-default: $text !important;\n")
            append("--text-strong: $text !important;\n")
            append("--text-subtle: $muted !important;\n")
            append("--text-brand: $accent !important;\n")
            append("--control-brand-foreground: $accent !important;\n")
            append("--control-brand-foreground-new: $accent !important;\n")
            append("--border-subtle: ${rgba(theme.text, 0.06)} !important;\n")
            append("--border-faint: ${rgba(theme.text, 0.04)} !important;\n")
            append("}\n")
            if (theme.id == "classic") {
                // 옛 글꼴과 네모에 가까운 모서리
                append(":root, .theme-dark { --font-primary: \"Whitney\", \"Helvetica Neue\", Helvetica, Arial, sans-serif !important;")
                append(" --font-display: \"Ginto\", \"Whitney\", \"Helvetica Neue\", Helvetica, Arial, sans-serif !important; }\n")
                append("body { font-family: var(--font-primary) !important; }\n")
            }
            // 멘션·링크·선택 표시도 강조색으로
            append("::selection { background: ${rgba(theme.accent, 0.35)}; }\n")
        }

        // 둥근 모서리 — 입력창, 팝업, 임베드
        append(
            """
            [class*="channelTextArea_"] [class*="scrollableContainer_"],
            [class*="channelTextArea_"] > div { border-radius: ${prefs.radius}px !important; }
            [class*="embedFull_"], [class*="popout_"], [class*="menu_"], [class*="modal_"],
            [class*="imageWrapper_"], [class*="attachment_"] { border-radius: ${prefs.radius}px !important; }
            """.trimIndent()
        )
        append("\n")

        // 휴대폰 폭에서는 서버 막대 + 채널 목록이 채팅을 거의 다 가린다. 숨길 수 있게.
        if (prefs.hideSidebar) {
            append(
                """
                nav[class*="guilds_"], [class^="guilds_"], [class*=" guilds_"],
                [class^="sidebar_"], [class*=" sidebar_"],
                [class^="sidebarList_"], [class*=" sidebarList_"] { display: none !important; }
                """.trimIndent()
            )
            append("\n")
        }
        if (prefs.hideMembers) {
            append("""[class^="membersWrap_"], [class*=" membersWrap_"], [class*="membersWrap"] { display: none !important; }""")
            append("\n")
        }
        // 광고성 요소 — Nitro 선물 단추, 스티커 권유
        if (prefs.hideNitro) {
            append(
                """
                button[aria-label="Send a gift"], button[aria-label="선물 보내기"],
                [class*="upsell"], [class*="premiumTab"] { display: none !important; }
                """.trimIndent()
            )
            append("\n")
        }
        if (prefs.customCss.isNotBlank()) {
            append("/* 사용자 CSS */\n")
            append(prefs.customCss)
            append("\n")
        }
    }
}
