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
            WindowManager.LayoutParams.FLAG_NOT_FOCUSABLE or WindowManager.LayoutParams.FLAG_LAYOUT_NO_LIMITS,
            PixelFormat.TRANSLUCENT,
        ).apply {
            gravity = Gravity.END or Gravity.TOP
            x = 0
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

    @SuppressLint("ClickableViewAccessibility")
    private fun showPopup() {
        if (popup != null) { closePopup(); return }

        val card = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            background = GradientDrawable().apply { setColor(CARD); cornerRadius = dp(18).toFloat() }
            setPadding(dp(8), dp(10), dp(8), dp(8))
            elevation = dp(8).toFloat()
        }
        card.addView(label("모드", 13f, MUTED, bold = true).apply { setPadding(dp(12), 0, dp(12), dp(6)) })

        if (inLockTask()) {
            // 고정 중엔 다른 모드로 보내지 않는다 — 고정의 뜻이 사라지므로.
            card.addView(label("📌 앱 고정 중\n고정 해제는 고정 화면의 [고정 해제]나\n뒤로 + 최근 앱 버튼을 함께 길게", 14f, Color.WHITE).apply {
                setPadding(dp(12), dp(8), dp(12), dp(12))
            })
        } else {
            val modes = store.value.modes
            val list = LinearLayout(this).apply { orientation = LinearLayout.VERTICAL }
            modes.forEach { mode ->
                list.addView(row(mode.type.emoji, mode.label) {
                    closePopup()
                    ModeLauncher.launch(this, mode)
                })
            }
            card.addView(ScrollView(this).apply { addView(list) }, LinearLayout.LayoutParams(-1, -2).apply { weight = 1f })
            card.addView(View(this).apply { setBackgroundColor(0x22FFFFFF) }, LinearLayout.LayoutParams(-1, dp(1)).apply {
                setMargins(dp(8), dp(6), dp(8), dp(6))
            })
            card.addView(row("⚙️", "설정") {
                closePopup()
                startActivity(Intent(this, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
            })
        }

        val screenH = screenHeight()
        val root = object : FrameLayout(this) {
            override fun dispatchKeyEvent(event: KeyEvent): Boolean {
                if (event.keyCode == KeyEvent.KEYCODE_BACK && event.action == KeyEvent.ACTION_UP) { closePopup(); return true }
                return super.dispatchKeyEvent(event)
            }
        }.apply {
            isFocusableInTouchMode = true
            setOnTouchListener { _, e -> if (e.action == MotionEvent.ACTION_DOWN) closePopup(); true } // 카드 바깥
        }
        val cardH = (screenH * 0.6f).roundToInt()
        val lp = FrameLayout.LayoutParams(dp(250), FrameLayout.LayoutParams.WRAP_CONTENT, Gravity.END or Gravity.TOP).apply {
            rightMargin = dp(store.value.sizeDp * 0.75f + 8)
            topMargin = (buttonParams.y + buttonParams.height / 2 - dp(120)).coerceIn(dp(24), (screenH - dp(300)).coerceAtLeast(dp(24)))
        }
        card.setOnTouchListener { _, _ -> false }
        card.isClickable = true
        root.addView(card, lp)
        card.post { if (card.height > cardH) card.layoutParams = lp.apply { height = cardH } }

        val params = WindowManager.LayoutParams(
            WindowManager.LayoutParams.MATCH_PARENT, WindowManager.LayoutParams.MATCH_PARENT,
            WindowManager.LayoutParams.TYPE_APPLICATION_OVERLAY,
            // 포커스를 받는다: 뒤로 키로 닫을 수 있게.
            WindowManager.LayoutParams.FLAG_DIM_BEHIND or WindowManager.LayoutParams.FLAG_LAYOUT_IN_SCREEN,
            PixelFormat.TRANSLUCENT,
        ).apply { dimAmount = 0.35f }
        popup = root
        wm.addView(root, params)
        root.requestFocus()
    }

    private fun closePopup() {
        popup?.let { runCatching { wm.removeView(it) } }
        popup = null
    }

    private fun label(text: String, size: Float, color: Int, bold: Boolean = false) = TextView(this).apply {
        this.text = text
        setTextColor(color)
        textSize = size
        if (bold) typeface = Typeface.DEFAULT_BOLD
    }

    private fun row(emoji: String, text: String, onClick: () -> Unit) = TextView(this).apply {
        this.text = "$emoji   $text"
        setTextColor(Color.WHITE)
        textSize = 16f
        gravity = Gravity.CENTER_VERTICAL
        minHeight = dp(48)
        setPadding(dp(12), 0, dp(12), 0)
        background = android.graphics.drawable.RippleDrawable(
            android.content.res.ColorStateList.valueOf(0x33FFFFFF),
            null,
            GradientDrawable().apply { setColor(Color.WHITE); cornerRadius = dp(10).toFloat() },
        )
        setOnClickListener { onClick() }
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
        private const val CARD = 0xF21B2230.toInt()
        private const val MUTED = 0xFF9AA4B8.toInt()

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
