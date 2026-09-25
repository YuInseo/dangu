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
import android.widget.ImageView
import android.widget.LinearLayout
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
        val view = FrameLayout(this).apply {
            contentDescription = "모드 버튼"
            addView(
                ImageView(context).apply {
                    setImageResource(R.drawable.ic_stat)
                    setColorFilter(Color.WHITE)
                },
                FrameLayout.LayoutParams(dp(20), dp(20), Gravity.CENTER),
            )
        }
        buttonParams = WindowManager.LayoutParams(
            dp(s.sizeDp * 0.75f), dp(s.sizeDp),
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

        // 세로로 끌어 옮기고, 거의 안 움직였으면 누른 것으로 본다.
        val slop = ViewConfiguration.get(this).scaledTouchSlop
        var downRawY = 0f
        var startY = 0
        var dragging = false
        view.setOnTouchListener { v, e ->
            when (e.actionMasked) {
                MotionEvent.ACTION_DOWN -> {
                    downRawY = e.rawY; startY = buttonParams.y; dragging = false
                    v.alpha = 1f
                }
                MotionEvent.ACTION_MOVE -> {
                    val dy = e.rawY - downRawY
                    if (!dragging && abs(dy) > slop) dragging = true
                    if (dragging) {
                        buttonParams.y = (startY + dy).roundToInt().coerceIn(0, screenHeight() - buttonParams.height)
                        wm.updateViewLayout(v, buttonParams)
                    }
                }
                MotionEvent.ACTION_UP, MotionEvent.ACTION_CANCEL -> {
                    v.alpha = store.value.alpha
                    if (dragging) {
                        // 옮긴 자리를 설정에도 남긴다(설정 화면의 슬라이더도 따라 움직인다).
                        val range = (screenHeight() - buttonParams.height).coerceAtLeast(1)
                        store.update { copy(y = buttonParams.y / range.toFloat()) }
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
        val w = dp(s.sizeDp * 0.75f)
        buttonParams.width = w
        buttonParams.height = h
        buttonParams.y = ((screenHeight() - h) * s.y).roundToInt()
        v.background = GradientDrawable().apply {
            setColor(ACCENT)
            // 오른쪽 끝에 붙은 반쪽 알약
            cornerRadii = floatArrayOf(h / 2f, h / 2f, 0f, 0f, 0f, 0f, h / 2f, h / 2f)
        }
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
    @SuppressLint("ClickableViewAccessibility")
    private fun showPopup() {
        if (popup != null) { closePopup(); return }

        val screenW = screenWidth()
        val screenH = screenHeight()
        val cx = screenW - buttonParams.width / 2f
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

        val root = object : FrameLayout(this) {
            override fun dispatchKeyEvent(event: KeyEvent): Boolean {
                if (event.keyCode == KeyEvent.KEYCODE_BACK && event.action == KeyEvent.ACTION_UP) { closePopup(); return true }
                return super.dispatchKeyEvent(event)
            }
        }.apply {
            isFocusableInTouchMode = true
            setOnTouchListener { _, e -> if (e.action == MotionEvent.ACTION_DOWN) closePopup(); true } // 거품 바깥
        }
        val dim = View(this).apply { setBackgroundColor(Color.BLACK); alpha = 0f }
        root.addView(dim, FrameLayout.LayoutParams(-1, -1))
        dim.animate().alpha(0.45f).setDuration(200).start()
        scrim = dim

        // 반지름과 펼칠 각도: 많을수록 크게.
        val n = entries.size
        val radius = (dp(96) + n * dp(14)).coerceAtMost(dp(190)).toFloat()
        val span = if (n <= 1) 0.0 else minOf(170.0, 34.0 * (n - 1))
        val margin = dp(64)
        fun ys(center: Double) = (0 until n).map { i ->
            val deg = center + span / 2 - (if (n <= 1) 0.0 else span * i / (n - 1))
            cy - radius * Math.sin(Math.toRadians(deg))
        }
        // 180°(왼쪽)을 가운데로, 잘리면 아래(>180)나 위(<180)로 돌린다.
        val center = (0..14).flatMap { k -> listOf(180.0 + k * 5, 180.0 - k * 5) }
            .firstOrNull { c -> ys(c).all { it in margin.toDouble()..(screenH - margin).toDouble() } } ?: 180.0

        val bubbleW = dp(92)
        val circle = dp(58)
        val made = ArrayList<Bubble>()
        entries.forEachIndexed { i, entry ->
            val deg = center + span / 2 - (if (n <= 1) 0.0 else span * i / (n - 1))
            val x = cx + radius * Math.cos(Math.toRadians(deg)).toFloat()
            val y = cy - radius * Math.sin(Math.toRadians(deg)).toFloat()
            val bubble = LinearLayout(this).apply {
                orientation = LinearLayout.VERTICAL
                gravity = Gravity.CENTER_HORIZONTAL
                contentDescription = entry.label
                addView(TextView(context).apply {
                    text = entry.emoji
                    textSize = 24f
                    gravity = Gravity.CENTER
                    background = GradientDrawable().apply {
                        shape = GradientDrawable.OVAL
                        colors = intArrayOf(0xFF2A3446.toInt(), 0xFF1B2230.toInt())
                        gradientType = GradientDrawable.RADIAL_GRADIENT
                        gradientRadius = circle / 1.2f
                        setStroke(dp(2), ACCENT)
                    }
                    elevation = dp(6).toFloat()
                }, LinearLayout.LayoutParams(circle, circle))
                addView(label(entry.label, 12f, Color.WHITE).apply {
                    gravity = Gravity.CENTER
                    setShadowLayer(6f, 0f, 1f, Color.BLACK)
                    maxLines = 2
                    setPadding(0, dp(4), 0, 0)
                }, LinearLayout.LayoutParams(bubbleW, LinearLayout.LayoutParams.WRAP_CONTENT))
                setOnClickListener { entry.action() }
            }
            val lp = FrameLayout.LayoutParams(bubbleW, FrameLayout.LayoutParams.WRAP_CONTENT).apply {
                leftMargin = (x - bubbleW / 2f).roundToInt()
                topMargin = (y - circle / 2f).roundToInt()
            }
            root.addView(bubble, lp)
            // 버튼 자리에서 튀어나오게: 처음엔 버튼 위치, 작고 투명하게.
            val dx = cx - x
            val dy = cy - y
            bubble.translationX = dx
            bubble.translationY = dy
            bubble.scaleX = 0.2f
            bubble.scaleY = 0.2f
            bubble.alpha = 0f
            bubble.animate()
                .translationX(0f).translationY(0f).scaleX(1f).scaleY(1f).alpha(1f)
                .setStartDelay(i * 28L)
                .setDuration(340)
                .setInterpolator(android.view.animation.OvershootInterpolator(1.4f))
                .start()
            made += Bubble(bubble, dx, dy)
        }

        // 가운데(버튼 자리)의 ✕
        val close = TextView(this).apply {
            text = "✕"
            textSize = 20f
            setTextColor(Color.WHITE)
            gravity = Gravity.CENTER
            contentDescription = "닫기"
            background = GradientDrawable().apply { shape = GradientDrawable.OVAL; setColor(ACCENT) }
            elevation = dp(8).toFloat()
            setOnClickListener { closePopup() }
            rotation = -90f
            animate().rotation(0f).setDuration(260).start()
        }
        val closeSize = dp(52)
        root.addView(close, FrameLayout.LayoutParams(closeSize, closeSize).apply {
            leftMargin = (screenW - closeSize - dp(6))
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
            b.view.animate().translationX(b.dx).translationY(b.dy).scaleX(0.2f).scaleY(0.2f).alpha(0f)
                .setStartDelay(0).setDuration(160).start()
        }
        scrim?.animate()?.alpha(0f)?.setDuration(160)?.start()
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
