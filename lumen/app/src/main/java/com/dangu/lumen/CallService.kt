package com.dangu.lumen

import android.Manifest
import android.annotation.SuppressLint
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.content.pm.ServiceInfo
import android.media.AudioAttributes
import android.media.AudioDeviceInfo
import android.media.AudioFocusRequest
import android.media.AudioManager
import android.os.Build
import android.os.IBinder
import android.os.PowerManager
import androidx.core.app.NotificationCompat
import androidx.core.app.ServiceCompat
import androidx.core.content.ContextCompat

/**
 * 통화 중에만 도는 포그라운드 서비스.
 *
 * 안드로이드는 화면이 꺼졌거나 뒤로 간 앱의 마이크를 막고, 곧 프로세스를 멈춘다. 이 서비스가 떠
 * 있는 동안은 "마이크를 쓰는 중인 앱"으로 취급되어 통화가 이어진다. 소리 길(스피커·수화부·이어폰)도
 * 여기서 통화용으로 잡는다.
 */
class CallService : Service() {
    private lateinit var audio: AudioManager
    private var focus: AudioFocusRequest? = null
    private var wakeLock: PowerManager.WakeLock? = null
    private var speaker = true

    override fun onBind(intent: Intent?): IBinder? = null

    override fun onCreate() {
        super.onCreate()
        audio = getSystemService(AudioManager::class.java)
        getSystemService(NotificationManager::class.java).createNotificationChannel(
            NotificationChannel(CHANNEL, "통화", NotificationManager.IMPORTANCE_LOW).apply {
                description = "음성·영상 통화 중 표시"
                setShowBadge(false)
            }
        )
    }

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        when (intent?.action) {
            ACTION_TOGGLE_SPEAKER -> {
                speaker = !speaker
                route()
                notifyState()
                return START_NOT_STICKY
            }
            ACTION_STOP -> {
                stopSelf()
                return START_NOT_STICKY
            }
        }

        if (ContextCompat.checkSelfPermission(this, Manifest.permission.RECORD_AUDIO) != PackageManager.PERMISSION_GRANTED) {
            stopSelf()
            return START_NOT_STICKY
        }
        val started = runCatching {
            ServiceCompat.startForeground(
                this, NOTIFICATION_ID, notification(),
                if (Build.VERSION.SDK_INT >= 30) ServiceInfo.FOREGROUND_SERVICE_TYPE_MICROPHONE else 0,
            )
        }.isSuccess
        if (!started) {
            stopSelf()
            return START_NOT_STICKY
        }
        startAudio()
        return START_NOT_STICKY
    }

    @SuppressLint("WakelockTimeout")
    private fun startAudio() {
        if (focus != null) return
        val attrs = AudioAttributes.Builder()
            .setUsage(AudioAttributes.USAGE_VOICE_COMMUNICATION)
            .setContentType(AudioAttributes.CONTENT_TYPE_SPEECH)
            .build()
        focus = AudioFocusRequest.Builder(AudioManager.AUDIOFOCUS_GAIN)
            .setAudioAttributes(attrs)
            .setOnAudioFocusChangeListener { }
            .build()
            .also { audio.requestAudioFocus(it) }
        audio.mode = AudioManager.MODE_IN_COMMUNICATION
        // 이어폰·블루투스가 있으면 그쪽, 없으면 스피커(디스코드 음성 채널은 보통 스피커로 듣는다).
        speaker = headset() == null
        route()

        wakeLock = getSystemService(PowerManager::class.java)
            .newWakeLock(PowerManager.PARTIAL_WAKE_LOCK, "lumen:call")
            .also { it.acquire() }
    }

    private fun headset(): AudioDeviceInfo? {
        val preferred = listOf(
            AudioDeviceInfo.TYPE_BLUETOOTH_SCO,
            AudioDeviceInfo.TYPE_BLE_HEADSET,
            AudioDeviceInfo.TYPE_WIRED_HEADSET,
            AudioDeviceInfo.TYPE_WIRED_HEADPHONES,
            AudioDeviceInfo.TYPE_USB_HEADSET,
        )
        val devices = if (Build.VERSION.SDK_INT >= 31) audio.availableCommunicationDevices
        else audio.getDevices(AudioManager.GET_DEVICES_OUTPUTS).toList()
        return preferred.firstNotNullOfOrNull { type -> devices.firstOrNull { it.type == type } }
    }

    @Suppress("DEPRECATION")
    private fun route() {
        val headset = headset()
        if (Build.VERSION.SDK_INT >= 31) {
            val devices = audio.availableCommunicationDevices
            val target = headset
                ?: devices.firstOrNull {
                    it.type == if (speaker) AudioDeviceInfo.TYPE_BUILTIN_SPEAKER else AudioDeviceInfo.TYPE_BUILTIN_EARPIECE
                }
            if (target != null) audio.setCommunicationDevice(target) else audio.clearCommunicationDevice()
        } else {
            audio.isSpeakerphoneOn = headset == null && speaker
        }
    }

    private fun notification() = NotificationCompat.Builder(this, CHANNEL)
        .setSmallIcon(R.drawable.ic_notification)
        .setContentTitle("통화 중")
        .setContentText(if (headset() != null) "이어폰으로 듣는 중" else if (speaker) "스피커" else "수화부")
        .setOngoing(true)
        .setCategory(NotificationCompat.CATEGORY_CALL)
        .setContentIntent(
            PendingIntent.getActivity(
                this, 0,
                Intent(this, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_SINGLE_TOP),
                PendingIntent.FLAG_IMMUTABLE,
            )
        )
        .addAction(
            0, if (speaker) "수화부로" else "스피커로",
            PendingIntent.getService(
                this, 1, Intent(this, CallService::class.java).setAction(ACTION_TOGGLE_SPEAKER),
                PendingIntent.FLAG_IMMUTABLE,
            ),
        )
        .build()

    private fun notifyState() {
        if (ContextCompat.checkSelfPermission(this, Manifest.permission.POST_NOTIFICATIONS) == PackageManager.PERMISSION_GRANTED ||
            Build.VERSION.SDK_INT < 33
        ) {
            getSystemService(NotificationManager::class.java).notify(NOTIFICATION_ID, notification())
        }
    }

    override fun onDestroy() {
        focus?.let { audio.abandonAudioFocusRequest(it) }
        focus = null
        runCatching {
            if (Build.VERSION.SDK_INT >= 31) audio.clearCommunicationDevice()
            @Suppress("DEPRECATION")
            audio.isSpeakerphoneOn = false
            audio.mode = AudioManager.MODE_NORMAL
        }
        wakeLock?.takeIf { it.isHeld }?.release()
        wakeLock = null
        super.onDestroy()
    }

    companion object {
        private const val CHANNEL = "call"
        private const val NOTIFICATION_ID = 42
        private const val ACTION_TOGGLE_SPEAKER = "com.dangu.lumen.TOGGLE_SPEAKER"
        private const val ACTION_STOP = "com.dangu.lumen.STOP_CALL"

        fun set(context: Context, active: Boolean) {
            val intent = Intent(context, CallService::class.java)
            if (active) runCatching { ContextCompat.startForegroundService(context, intent) }
            else context.stopService(intent)
        }
    }
}
