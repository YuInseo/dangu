package com.dangu.modes.ui

import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.darkColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.ui.graphics.Color

val Accent = Color(0xFF5B8CFF)

@Composable
fun ModesTheme(content: @Composable () -> Unit) {
    MaterialTheme(
        colorScheme = darkColorScheme(
            primary = Accent,
            background = Color(0xFF101318),
            surface = Color(0xFF101318),
            surfaceContainer = Color(0xFF171B23),
            surfaceContainerHigh = Color(0xFF1E2430),
            surfaceContainerHighest = Color(0xFF262D3B),
        ),
        content = content,
    )
}
