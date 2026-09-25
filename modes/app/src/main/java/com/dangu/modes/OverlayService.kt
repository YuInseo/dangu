package com.dangu.modes

import android.annotation.SuppressLint
import android.app.ActivityManager
import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.content.res.Configuration
import android.graphics.Color
import android.graphics.PixelFormat
import android.graphics.Typeface
import android.graphics.drawable.GradientDrawable
import android.os.Build
import android.os.IBinder
import android.provider.Settings
import android.util.TypedValue
import android.view.Gravity
import android.view.KeyEvent
import android.view.MotionEvent
import android.view.View
import android.view.ViewConfiguration
import android.view.WindowManager
import android.widget.FrameLayout
import android.widget.LinearLayout
import android.widget.ScrollView
import android.widget.TextView
import androidx.core.app.NotificationCompat
import androidx.core.app.ServiceCompat
import androidx.core.content.ContextCompat
import kotlinx.coroutines.MainScope
import kotlinx.coroutines.cancel
import kotlinx.coroutines.launch
import kotlin.math.abs
import kotlin.math.roundToInt

/**
 * 모든 화면 위에 떠 있는 플로팅 버튼.
 *
 * 창은 둘이다. 오른쪽 끝의 버튼 창(작고, 포커스를 받지 않아 뒤의 앱 조작을 막지 않는다)과, 누르면
 * 뜨는 팝업 창(화면 전체, 포커스를 받아 뒤로 키로 닫힌다). 설정 화면이 [Store]를 바꾸면 여기서
 * 듣고 updateViewLayout()으로 위치·크기·투명도를 즉시 바꾼다 — 서비스를 다시 띄울 필요가 없다.
 */
class OverlayService : Service() {
    private val scope = MainScope()
    private lateinit var wm: WindowManager
    private lateinit var store: Store

    private var button: FrameLayout? = null
    private lateinit var buttonParams: WindowManager.LayoutParams
    private var popup: View? = null

    override fun onBind(intent: Intent?): IBinder? = null

    override fun onCreate() {
        super.onCreate()
        wm = getSystemService(WindowManager::class.java)
        store = Store.get(this)
    }

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        if (intent?.action == ACTION_HIDE) {
            store.update { copy(enabled = false) }
            stopSelf()
            return START_NOT_STICKY
        }
        startInForeground()
        if (!Settings.canDrawOverlays(this)) {
            stopSelf()
            return START_NOT_STICKY
        }
        if (button == null) {
            addButton()
            // 버튼이 떠 있는 동안 반나절에 한 번 새 버전 확인(있으면 받아 두고 알림).
            scope.launch {
                while (true) {
                    runCatching { AutoUpdate.run(this@OverlayService) }
                    kotlinx.coroutines.delay(60 * 60 * 1000L)
                }
            }
            scope.launch {
                store.state.collect { s ->
                    if (!s.enabled) stopSelf() else apply(s)
                }
            }
        }
        return START_STICKY
    }

    private fun startInForeground() {
        val nm = getSystemService(NotificationManager::class.java)
        nm.createNotificationChannel(
            NotificationChannel(CHANNEL, "플로팅 버튼", NotificationManager.IMPORTANCE_MIN).apply { setShowBadge(false) }
        )
        val settings = PendingIntent.getActivity(
            this, 0, Intent(this, MainActivity::class.java), PendingIntent.FLAG_IMMUTABLE,
        )
        val hide = PendingIntent.getService(
            this, 1, Intent(this, OverlayService::class.java).setAction(ACTION_HIDE), PendingIntent.FLAG_IMMUTABLE,
        )
        val n: Notification = NotificationCompat.Builder(this, CHANNEL)
            .setSmallIcon(R.drawable.ic_stat)
            .setContentTitle("모드 버튼 표시 중")
            .setContentText("눌러서 설정")
            .setContentIntent(settings)
            .addAction(0, "숨기기", hide)
            .setOngoing(true)
            .setPriority(NotificationCompat.PRIORITY_MIN)
            .build()
        ServiceCompat.startForeground(
            this, NOTIFICATION_ID, n,
            if (Build.VERSION.SDK_INT >= 34) ServiceInfo.FOREGROUND_SERVICE_TYPE_SPECIAL_USE else 0,
        )
    }

    private fun dp(v: Number) = TypedValue.applyDimension(TypedValue.COMPLEX_UNIT_DIP, v.toFloat(), resources.displayMetrics).roundToInt()

    private fun screenHeight(): Int =
        if (Build.VERSION.SDK_INT >= 30) wm.currentWindowMetrics.bounds.height() else resources.displayMetrics.heightPixels

    private fun screenWidth(): Int =
        if (Build.VERSION.SDK_INT >= 30) wm.currentWindowMetrics.bounds.width() else resources.displayMetrics.widthPixels

    // ───────── 버튼 ─────────

    @SuppressLint("ClickableViewAccessibility")
    private fun addButton() {
        val s = store.value
        // 화면 끝의 얇은 막대. 보이는 막대는 가늘어도 누르는 자리는 넉넉하게(투명한 여백).
        val view = FrameLayout(this).apply {
            contentDescription = "모드 버튼"
            addView(View(context), FrameLayout.LayoutParams(dp(s.thickDp), -1, Gravity.END))
        }
        buttonParams = WindowManager.LayoutParams(
            dp(s.thickDp + TOUCH_EXTRA), dp(s.sizeDp),
            WindowManager.LayoutParams.TYPE_APPLICATION_OVERLAY,
            // 포커스를 받지 않는다: 버튼 밖의 터치·키는 뒤의 앱이 그대로 받는다.
            // LAYOUT_IN_SCREEN: y를 상태 표시줄 아래가 아니라 화면 맨 위부터 잰다 — 설정의 "세로 위치"와 맞게.
            WindowManager.LayoutParams.FLAG_NOT_FOCUSABLE or
                WindowManager.LayoutParams.FLAG_LAYOUT_IN_SCREEN or
                WindowManager.LayoutParams.FLAG_LAYOUT_NO_LIMITS,
            PixelFormat.TRANSLUCENT,
        ).apply {
            gravity = Gravity.END or Gravity.TOP
            x = 0
            if (Build.VERSION.SDK_INT >= 30) setFitInsetsTypes(0)
        }

        // 끌어서 옮기고(세로는 자리, 가로로 화면 가운데를 넘기면 반대쪽 끝으로), 거의 안 움직였으면 누른 것.
        val slop = ViewConfiguration.get(this).scaledTouchSlop
        var downRawX = 0f
        var downRawY = 0f
        var startY = 0
        var dragging = false
        view.setOnTouchListener { v, e ->
            when (e.actionMasked) {
                MotionEvent.ACTION_DOWN -> {
                    downRawX = e.rawX; downRawY = e.rawY; startY = buttonParams.y; dragging = false
                    v.alpha = 1f
                }
                MotionEvent.ACTION_MOVE -> {
                    val dy = e.rawY - downRawY
                    val dx = e.rawX - downRawX
                    if (!dragging && (abs(dy) > slop || abs(dx) > slop)) dragging = true
                    if (dragging) {
                        buttonParams.y = (startY + dy).roundToInt().coerceIn(0, screenHeight() - buttonParams.height)
                        // 끄는 동안 손가락을 따라 가로로도 움직인다(붙은 쪽 끝에서 잰 거리).
                        buttonParams.x = (if (store.value.right) -dx else dx).roundToInt().coerceIn(0, screenWidth() - buttonParams.width)
                        wm.updateViewLayout(v, buttonParams)
                    }
                }
                MotionEvent.ACTION_UP, MotionEvent.ACTION_CANCEL -> {
                    v.alpha = store.value.alpha
                    if (dragging) {
                        // 옮긴 자리를 설정에도 남긴다(설정 화면의 슬라이더도 따라 움직인다).
                        val range = (screenHeight() - buttonParams.height).coerceAtLeast(1)
                        val flip = e.rawX < screenWidth() / 2f == store.value.right
                        buttonParams.x = 0
                        store.update { copy(y = buttonParams.y / range.toFloat(), right = if (flip) !right else right) }
                    } else if (e.actionMasked == MotionEvent.ACTION_UP) {
                        v.performClick()
                        showPopup()
                    }
                }
            }
            true
        }
        button = view
        wm.addView(view, buttonParams)
        apply(s)
    }

    /** 설정 값을 버튼 창에 반영한다. 설정 화면에서 슬라이더를 움직이면 바로 여기로 온다. */
    private fun apply(s: Store.Snapshot) {
        val v = button ?: return
        val h = dp(s.sizeDp)
        buttonParams.width = dp(s.thickDp + TOUCH_EXTRA)
        buttonParams.height = h
        buttonParams.y = ((screenHeight() - h) * s.y).roundToInt()
        buttonParams.gravity = (if (s.right) Gravity.END else Gravity.START) or Gravity.TOP
        val bar = v.getChildAt(0)
        bar.layoutParams = FrameLayout.LayoutParams(dp(s.thickDp), -1, if (s.right) Gravity.END else Gravity.START).apply {
            // 끝에서 살짝 띄운다
            if (s.right) rightMargin = dp(2) else leftMargin = dp(2)
        }
        bar.background = GradientDrawable().apply {
            setColor(s.color)
            cornerRadius = dp(s.thickDp).toFloat()
        }
        bar.elevation = dp(2).toFloat()
        v.alpha = s.alpha
        runCatching { wm.updateViewLayout(v, buttonParams) }
    }

    override fun onConfigurationChanged(newConfig: Configuration) {
        super.onConfigurationChanged(newConfig)
        apply(store.value) // 회전하면 화면 높이가 바뀐다 — 비율로 다시 놓는다.
    }

    // ───────── 모드 목록 팝업 ─────────

    private fun inLockTask(): Boolean =
        getSystemService(ActivityManager::class.java).lockTaskModeState != ActivityManager.LOCK_TASK_MODE_NONE

    private class Bubble(val view: View, val dx: Float, val dy: Float)
    private var bubbles: List<Bubble> = emptyList()
    private var scrim: View? = null

    /**
     * Circle처럼: 버튼을 중심으로 반원을 그리며 모드들이 동그란 거품으로 튀어나온다.
     * 버튼이 화면 위·아래 끝에 가까우면 반원을 아래·위로 돌려 잘리지 않게 한다.
     */
    /**
     * 엣지 패널처럼: 막대 쪽 끝에서 키 큰 둥근 패널이 미끄러져 나오고, 모드들이 큰 타일과 이름으로
     * 세로로 늘어선다(많으면 굴린다). 앱을 여는 모드는 그 앱의 진짜 아이콘. 아래에 설정·편집.
     */
    @SuppressLint("ClickableViewAccessibility")
    private fun showPanel() {
        val screenW = screenWidth()
        val screenH = screenHeight()
        val right = store.value.right
        panelOnRight = right

        val root = object : FrameLayout(this) {
            override fun dispatchKeyEvent(event: KeyEvent): Boolean {
                if (event.keyCode == KeyEvent.KEYCODE_BACK && event.action == KeyEvent.ACTION_UP) { closePopup(); return true }
                return super.dispatchKeyEvent(event)
            }
        }.apply {
            isFocusableInTouchMode = true
            setOnTouchListener { _, e -> if (e.action == MotionEvent.ACTION_DOWN) closePopup(); true }
        }
        val dim = View(this).apply { setBackgroundColor(Color.BLACK); alpha = 0f }
        root.addView(dim, FrameLayout.LayoutParams(-1, -1))
        dim.animate().alpha(0.18f).setDuration(200).start()
        scrim = dim

        val width = dp(118)
        val card = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            background = GradientDrawable().apply { setColor(0xD9262629.toInt()); cornerRadius = dp(30).toFloat() }
            elevation = dp(10).toFloat()
            isClickable = true // 패널 안을 누른 건 바깥 누름이 아니다
        }

        val list = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            gravity = Gravity.CENTER_HORIZONTAL
            setPadding(0, dp(18), 0, dp(12))
        }
        val tile = dp(62)
        fun addEntry(iconView: View, text: String, onClick: () -> Unit) {
            val item = LinearLayout(this).apply {
                orientation = LinearLayout.VERTICAL
                gravity = Gravity.CENTER_HORIZONTAL
                contentDescription = text
                setPadding(0, dp(10), 0, dp(10))
                background = android.graphics.drawable.RippleDrawable(
                    android.content.res.ColorStateList.valueOf(0x33FFFFFF), null,
                    GradientDrawable().apply { setColor(Color.WHITE); cornerRadius = dp(18).toFloat() },
                )
                setOnClickListener { onClick() }
                addView(iconView, LinearLayout.LayoutParams(tile, tile))
                addView(label(text, 13f, Color.WHITE).apply {
                    gravity = Gravity.CENTER
                    maxLines = 2
                    setPadding(dp(4), dp(6), dp(4), 0)
                }, LinearLayout.LayoutParams(-1, LinearLayout.LayoutParams.WRAP_CONTENT))
            }
            list.addView(item, LinearLayout.LayoutParams(-1, LinearLayout.LayoutParams.WRAP_CONTENT).apply {
                setMargins(dp(8), dp(2), dp(8), dp(2))
            })
        }
        fun emojiTile(emoji: String) = TextView(this).apply {
            text = emoji
            textSize = 28f
            gravity = Gravity.CENTER
            background = GradientDrawable().apply { setColor(0xFFF4F4F6.toInt()); cornerRadius = dp(18).toFloat() }
        }
        fun appTile(pkg: String): View = runCatching {
            android.widget.ImageView(this).apply {
                setImageDrawable(packageManager.getApplicationIcon(pkg))
                scaleType = android.widget.ImageView.ScaleType.FIT_CENTER
            }
        }.getOrElse { emojiTile("📱") }

        if (inLockTask()) {
            addEntry(emojiTile("📌"), "앱 고정 중\n고정 화면에서 해제") { closePopup() }
        } else {
            store.value.modes.forEachIndexed { i, m ->
                addEntry(if (m.needsApp && m.packageName.isNotBlank()) appTile(m.packageName) else emojiTile(m.type.emoji), m.label) {
                    closePopup(); ModeLauncher.launch(this, m)
                }
                // 엣지 패널처럼 첫 줄(바탕화면들)과 나머지 사이에 점선
                if (i == 1 && store.value.modes.size > 2) {
                    list.addView(TextView(this).apply {
                        text = "· · · · · · · · · ·"
                        setTextColor(0x88FFFFFF.toInt())
                        gravity = Gravity.CENTER
                    }, LinearLayout.LayoutParams(-1, dp(22)))
                }
            }
        }
        card.addView(ScrollView(this).apply {
            isVerticalScrollBarEnabled = false
            overScrollMode = View.OVER_SCROLL_NEVER
            addView(list)
        }, LinearLayout.LayoutParams(-1, 0, 1f))

        // 아래: 설정 · 편집
        val bottom = LinearLayout(this).apply {
            orientation = LinearLayout.HORIZONTAL
            gravity = Gravity.CENTER
            setPadding(0, dp(6), 0, dp(14))
        }
        fun bottomButton(glyph: String, desc: String, onClick: () -> Unit) = TextView(this).apply {
            text = glyph
            textSize = 20f
            setTextColor(Color.WHITE)
            gravity = Gravity.CENTER
            contentDescription = desc
            setOnClickListener { onClick() }
        }
        val openSettings = {
            closePopup()
            startActivity(Intent(this, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
        }
        bottom.addView(bottomButton("⚙", "설정", openSettings), LinearLayout.LayoutParams(dp(48), dp(40)))
        bottom.addView(bottomButton("✎", "모드 편집", openSettings), LinearLayout.LayoutParams(dp(48), dp(40)))
        card.addView(bottom, LinearLayout.LayoutParams(-1, LinearLayout.LayoutParams.WRAP_CONTENT))

        val top = dp(48)
        val height = (screenH * 0.78f).roundToInt().coerceAtMost(screenH - top - dp(40))
        root.addView(card, FrameLayout.LayoutParams(width, height).apply {
            gravity = (if (right) Gravity.END else Gravity.START) or Gravity.TOP
            if (right) rightMargin = dp(10) else leftMargin = dp(10)
            topMargin = top
        })
        // 끝에서 미끄러져 나온다
        card.translationX = (if (right) 1 else -1) * (width + dp(20)).toFloat()
        card.animate().translationX(0f).setDuration(240)
            .setInterpolator(android.view.animation.DecelerateInterpolator(1.6f)).start()
        panelCard = card

        val params = WindowManager.LayoutParams(
            WindowManager.LayoutParams.MATCH_PARENT, WindowManager.LayoutParams.MATCH_PARENT,
            WindowManager.LayoutParams.TYPE_APPLICATION_OVERLAY,
            WindowManager.LayoutParams.FLAG_LAYOUT_IN_SCREEN or WindowManager.LayoutParams.FLAG_LAYOUT_NO_LIMITS,
            PixelFormat.TRANSLUCENT,
        ).apply {
            gravity = Gravity.START or Gravity.TOP
            if (Build.VERSION.SDK_INT >= 30) setFitInsetsTypes(0)
            // 뒤를 흐리게(기기가 지원하면 — 엣지 패널처럼)
            if (Build.VERSION.SDK_INT >= 31) {
                flags = flags or WindowManager.LayoutParams.FLAG_BLUR_BEHIND
                blurBehindRadius = dp(18)
            }
        }
        popup = root
        button?.visibility = View.INVISIBLE
        wm.addView(root, params)
        root.requestFocus()
    }

    /** 엣지 패널이 열려 있으면 그 패널(닫을 때 미끄러져 나간다). */
    private var panelCard: View? = null
    private var panelOnRight = true

    @SuppressLint("ClickableViewAccessibility")
    private fun showPopup() {
        if (popup != null) { closePopup(); return }
        if (store.value.panel) { showPanel(); return }

        val screenW = screenWidth()
        val screenH = screenHeight()
        val right = store.value.right
        val cx = if (right) screenW - buttonParams.width / 2f else buttonParams.width / 2f
        val cy = buttonParams.y + buttonParams.height / 2f

        data class Entry(val emoji: String, val label: String, val action: () -> Unit)
        val entries = if (inLockTask()) {
            // 고정 중엔 다른 모드로 보내지 않는다 — 고정의 뜻이 사라지므로.
            listOf(Entry("📌", "앱 고정 중\n(고정 화면에서 해제)") { closePopup() })
        } else {
            store.value.modes.map { m -> Entry(m.type.emoji, m.label) { closePopup(); ModeLauncher.launch(this, m) } } +
                Entry("⚙️", "설정") {
                    closePopup()
                    startActivity(Intent(this, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
                }
        }

        // 거품을 잡고 돌려도 돌아가야 한다: 손가락이 움직이기 시작하면 거품 대신 이 판이 가로챈다.
        var intercept: (MotionEvent) -> Boolean = { false }
        val root = object : FrameLayout(this) {
            override fun dispatchKeyEvent(event: KeyEvent): Boolean {
                if (event.keyCode == KeyEvent.KEYCODE_BACK && event.action == KeyEvent.ACTION_UP) { closePopup(); return true }
                return super.dispatchKeyEvent(event)
            }
            override fun onInterceptTouchEvent(ev: MotionEvent): Boolean = intercept(ev)
        }.apply { isFocusableInTouchMode = true }
        val dim = View(this).apply { setBackgroundColor(Color.BLACK); alpha = 0f }
        root.addView(dim, FrameLayout.LayoutParams(-1, -1))
        dim.animate().alpha(0.45f).setDuration(200).start()
        scrim = dim

        // 이웃 거품 사이(중심끼리) 거리를 고정하고, "한 번에 보일 개수"가 반원(최대 160°)에 들어갈 만큼만 반지름을 키운다.
        val n = entries.size
        val visible = minOf(n, store.value.maxVisible).coerceAtLeast(1)
        // 알약은 가로로 길어서, 호가 가파르면(위·아래 끝) 이웃끼리 겹친다. 호를 완만하게(반지름을 크게,
        // 펼치는 각도는 작게) 해서 위아래 간격이 거의 일정하게 한다.
        val chord = dp(62).toDouble()
        val maxSpan = 110.0
        val radius = if (visible <= 1) dp(96).toFloat() else {
            val stepMax = Math.toRadians(minOf(30.0, maxSpan / (visible - 1)))
            maxOf(dp(150).toDouble(), chord / (2 * Math.sin(stepMax / 2))).toFloat()
        }
        val step = if (n <= 1) 0.0 else Math.toDegrees(2 * Math.asin((chord / (2 * radius)).coerceAtMost(1.0)))
        val window = step * (visible - 1) // 한 번에 보이는 각도
        val margin = dp(64).toDouble()

        // 각도는 "버튼에서 화면 안쪽으로" 잰다: 180°가 정면(가로), 90°가 위, 270°가 아래.
        // 항목 i의 각도 = base + i·step. 화면 끝에 가까우면 보이는 창을 위·아래로 돌려 잘리지 않게.
        fun yAt(deg: Double) = cy - radius * Math.sin(Math.toRadians(deg))
        val windowCenter = (0..14).flatMap { k -> listOf(180.0 + k * 5, 180.0 - k * 5) }
            .firstOrNull { c -> listOf(c - window / 2, c + window / 2).all { yAt(it) in margin..(screenH - margin) } } ?: 180.0
        val lo = windowCenter - window / 2 // 보이는 창의 위 끝
        val hi = windowCenter + window / 2 // 아래 끝
        val total = step * (n - 1)
        // 돌릴 수 있는 범위: 첫 항목이 창의 위 끝 ~ 마지막 항목이 창의 아래 끝.
        val baseMin = hi - total
        val baseMax = lo
        var base = if (n <= visible) windowCenter - total / 2 else baseMax

        fun offsetOf(deg: Double): Pair<Float, Float> {
            val dx = (radius * Math.cos(Math.toRadians(deg))).toFloat() // 180°면 -radius(안쪽)
            val dy = (-radius * Math.sin(Math.toRadians(deg))).toFloat()
            return (if (right) dx else -dx) to dy
        }

        val circle = dp(50)
        val accent = store.value.color.let { if (it == 0xFFFFFFFF.toInt()) ACCENT else it }
        val made = ArrayList<Bubble>()
        val views = ArrayList<View>()

        /** 각 거품을 지금 base에 맞는 자리로. 창 밖으로 나간 것은 흐려지고 눌리지 않는다. */
        fun layoutAll(animate: Boolean) {
            views.forEachIndexed { i, v ->
                val deg = base + i * step
                val (ox, oy) = offsetOf(deg)
                val out = maxOf(lo - deg, deg - hi, 0.0)
                val fade = (1.0 - out / (step * 0.9)).coerceIn(0.0, 1.0).toFloat()
                v.isEnabled = fade > 0.5f
                if (animate) return@forEachIndexed
                v.translationX = ox
                v.translationY = oy
                v.alpha = fade
                val sc = 0.75f + 0.25f * fade
                v.scaleX = sc; v.scaleY = sc
            }
        }

        entries.forEachIndexed { i, entry ->
            val icon = TextView(this).apply {
                text = entry.emoji
                textSize = 22f
                gravity = Gravity.CENTER
                background = GradientDrawable().apply {
                    shape = GradientDrawable.OVAL
                    colors = intArrayOf(0xFF2A3446.toInt(), 0xFF1B2230.toInt())
                    gradientType = GradientDrawable.RADIAL_GRADIENT
                    gradientRadius = circle / 1.2f
                    setStroke(dp(2), accent)
                }
            }
            val name = label(entry.label, 14f, Color.WHITE).apply {
                gravity = Gravity.CENTER_VERTICAL
                maxLines = 2
                setPadding(dp(12), 0, dp(12), 0)
            }
            // 원은 바깥쪽(버튼 쪽), 이름은 화면 안쪽으로. 둘을 감싼 알약 배경.
            val bubble = LinearLayout(this).apply {
                orientation = LinearLayout.HORIZONTAL
                gravity = Gravity.CENTER_VERTICAL
                contentDescription = entry.label
                background = GradientDrawable().apply { setColor(0xE61B2230.toInt()); cornerRadius = circle / 2f }
                elevation = dp(6).toFloat()
                if (right) {
                    addView(name, LinearLayout.LayoutParams(LinearLayout.LayoutParams.WRAP_CONTENT, circle))
                    addView(icon, LinearLayout.LayoutParams(circle, circle))
                } else {
                    addView(icon, LinearLayout.LayoutParams(circle, circle))
                    addView(name, LinearLayout.LayoutParams(LinearLayout.LayoutParams.WRAP_CONTENT, circle))
                }
                setOnClickListener { if (isEnabled) entry.action() }
            }
            // 기준점은 버튼 중심: 원의 중심이 거기 오도록 붙는 쪽 끝에서 잰다. 자리는 translation으로.
            root.addView(bubble, FrameLayout.LayoutParams(FrameLayout.LayoutParams.WRAP_CONTENT, circle).apply {
                gravity = (if (right) Gravity.END else Gravity.START) or Gravity.TOP
                if (right) rightMargin = (screenW - cx - circle / 2f).roundToInt()
                else leftMargin = (cx - circle / 2f).roundToInt()
                topMargin = (cy - circle / 2f).roundToInt()
            })
            views += bubble
        }
        layoutAll(animate = false)
        views.forEach { v -> v.post { v.pivotX = if (right) v.width - circle / 2f else circle / 2f; v.pivotY = circle / 2f } }
        // 버튼 자리에서 튀어나오게.
        views.forEachIndexed { i, v ->
            val tx = v.translationX; val ty = v.translationY; val a = v.alpha; val sc = v.scaleX
            v.translationX = 0f; v.translationY = 0f; v.scaleX = 0.2f; v.scaleY = 0.2f; v.alpha = 0f
            v.animate().translationX(tx).translationY(ty).scaleX(sc).scaleY(sc).alpha(a)
                .setStartDelay(i * 28L).setDuration(340)
                .setInterpolator(android.view.animation.OvershootInterpolator(1.4f)).start()
            made += Bubble(v, 0f, 0f)
        }

        // 돌리기: 버튼 중심을 축으로 손가락이 도는 각도만큼 base를 돌린다. 놓으면 가장 가까운 칸에 딱 맞춘다.
        // 움직이지 않고 떼면(바깥을 누름) 닫는다.
        val slop = ViewConfiguration.get(this).scaledTouchSlop
        var downX = 0f; var downY = 0f; var lastAngle = 0.0; var rotating = false
        fun angleOf(x: Float, y: Float): Double {
            val dx = if (right) x - cx else cx - x
            return Math.toDegrees(Math.atan2(-(y - cy).toDouble(), dx.toDouble()))
        }
        intercept = { e ->
            when (e.actionMasked) {
                MotionEvent.ACTION_DOWN -> { downX = e.x; downY = e.y; lastAngle = angleOf(e.x, e.y); rotating = false; false }
                MotionEvent.ACTION_MOVE ->
                    n > visible && Math.hypot((e.x - downX).toDouble(), (e.y - downY).toDouble()) > slop
                else -> false
            }
        }
        root.setOnTouchListener { _, e ->
            when (e.actionMasked) {
                MotionEvent.ACTION_DOWN -> { downX = e.x; downY = e.y; lastAngle = angleOf(e.x, e.y); rotating = false }
                MotionEvent.ACTION_MOVE -> {
                    if (!rotating && Math.hypot((e.x - downX).toDouble(), (e.y - downY).toDouble()) > slop) rotating = n > visible
                    if (rotating) {
                        val a2 = angleOf(e.x, e.y)
                        var d = a2 - lastAngle
                        if (d > 180) d -= 360
                        if (d < -180) d += 360
                        lastAngle = a2
                        base = (base + d).coerceIn(baseMin, baseMax)
                        views.forEach { it.animate().cancel() }
                        layoutAll(animate = false)
                    }
                }
                MotionEvent.ACTION_UP -> {
                    if (rotating) {
                        // 가장 가까운 칸으로 스르륵
                        val snapped = (baseMax - Math.round((baseMax - base) / step) * step).coerceIn(baseMin, baseMax)
                        val from = base
                        android.animation.ValueAnimator.ofFloat(0f, 1f).apply {
                            duration = 160
                            addUpdateListener { va -> base = from + (snapped - from) * (va.animatedValue as Float); layoutAll(animate = false) }
                            start()
                        }
                    } else if (Math.hypot((e.x - downX).toDouble(), (e.y - downY).toDouble()) <= slop) {
                        closePopup()
                    }
                }
            }
            true
        }

        // 가운데(버튼 자리)의 ✕
        val close = TextView(this).apply {
            text = "✕"
            textSize = 20f
            setTextColor(Color.WHITE)
            gravity = Gravity.CENTER
            contentDescription = "닫기"
            background = GradientDrawable().apply { shape = GradientDrawable.OVAL; setColor(accent) }
            elevation = dp(8).toFloat()
            setOnClickListener { closePopup() }
            rotation = -90f
            animate().rotation(0f).setDuration(260).start()
        }
        val closeSize = dp(52)
        root.addView(close, FrameLayout.LayoutParams(closeSize, closeSize).apply {
            leftMargin = if (right) screenW - closeSize - dp(6) else dp(6)
            topMargin = (cy - closeSize / 2f).roundToInt()
        })
        bubbles = made

        val params = WindowManager.LayoutParams(
            WindowManager.LayoutParams.MATCH_PARENT, WindowManager.LayoutParams.MATCH_PARENT,
            WindowManager.LayoutParams.TYPE_APPLICATION_OVERLAY,
            // 포커스를 받는다: 뒤로 키로 닫을 수 있게. 좌표는 버튼과 같이 화면 맨 위부터.
            WindowManager.LayoutParams.FLAG_LAYOUT_IN_SCREEN or WindowManager.LayoutParams.FLAG_LAYOUT_NO_LIMITS,
            PixelFormat.TRANSLUCENT,
        ).apply {
            gravity = Gravity.START or Gravity.TOP
            if (Build.VERSION.SDK_INT >= 30) setFitInsetsTypes(0)
        }
        popup = root
        button?.visibility = View.INVISIBLE
        wm.addView(root, params)
        root.requestFocus()
    }

    /** 거품들이 버튼 자리로 빨려 들어간 뒤 창을 치운다. */
    private fun closePopup() {
        val root = popup ?: return
        popup = null
        bubbles.forEach { b ->
            b.view.animate().translationX(0f).translationY(0f).scaleX(0.2f).scaleY(0.2f).alpha(0f)
                .setStartDelay(0).setDuration(160).start()
        }
        scrim?.animate()?.alpha(0f)?.setDuration(160)?.start()
        panelCard?.let { c ->
            c.animate().translationX((if (panelOnRight) 1 else -1) * (c.width + dp(20)).toFloat()).setDuration(170).start()
        }
        panelCard = null
        bubbles = emptyList()
        root.postDelayed({
            runCatching { wm.removeView(root) }
            button?.visibility = View.VISIBLE
        }, 170)
    }

    private fun label(text: String, size: Float, color: Int, bold: Boolean = false) = TextView(this).apply {
        this.text = text
        setTextColor(color)
        textSize = size
        if (bold) typeface = Typeface.DEFAULT_BOLD
    }

    override fun onDestroy() {
        scope.cancel()
        closePopup()
        button?.let { runCatching { wm.removeView(it) } }
        button = null
        super.onDestroy()
    }

    companion object {
        private const val CHANNEL = "overlay"
        private const val NOTIFICATION_ID = 7
        private const val ACTION_HIDE = "com.dangu.modes.HIDE"
        private const val ACCENT = 0xFF5B8CFF.toInt()
        /** 막대 옆 투명한 누르는 자리(dp) */
        private const val TOUCH_EXTRA = 18

        /** 버튼 켜기. 권한이 없으면 false — 설정 화면이 권한 화면으로 보낸다. */
        fun start(context: Context): Boolean {
            if (!Settings.canDrawOverlays(context)) return false
            ContextCompat.startForegroundService(context, Intent(context, OverlayService::class.java))
            return true
        }

        fun stop(context: Context) {
            context.stopService(Intent(context, OverlayService::class.java))
        }
    }
}
