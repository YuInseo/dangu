#!/usr/bin/env python3
"""`tauri android init`이 만든 안드로이드 프로젝트를 소리에 맞게 고친다.

- 음악 폴더를 읽을 권한(READ_MEDIA_AUDIO, 표지 그림용 READ_MEDIA_IMAGES)을 매니페스트에 넣고,
  처음 켤 때 묻는다.
- 창은 앱 안에서 뜬 소리 서버(http://127.0.0.1:4380)를 연다. 릴리스 빌드에서도 이 평문
  주소를 열 수 있게 한다(밖으로 나가는 주소가 아니다).
- 안드로이드 15부터는 화면이 상태 표시줄·제스처 막대 밑까지 그려진다(edge-to-edge). 화면이
  그 밑에 깔리지 않게 시스템 막대와 키보드 자리만큼 띄우고, 막대 자리는 앱의 어두운 바탕으로 둔다.
- 자동 업데이트: 소리 서버가 받아 둔 새 APK를 시스템 설치 창으로 연다(UpdatePlugin, FileProvider,
  REQUEST_INSTALL_PACKAGES).
- 라이브 공연 저장: 음악 폴더에 쓴다. 안드로이드 10 이하에서만 쓰기 권한(WRITE_EXTERNAL_STORAGE,
  requestLegacyExternalStorage)이 필요하다.
"""
import pathlib
import re
import sys

SERVICE = 'package __PKG__\n\nimport android.app.Notification\nimport android.app.NotificationChannel\nimport android.app.NotificationManager\nimport android.app.PendingIntent\nimport android.app.Service\nimport android.content.BroadcastReceiver\nimport android.content.Context\nimport android.content.Intent\nimport android.content.IntentFilter\nimport android.content.pm.ServiceInfo\nimport android.graphics.Bitmap\nimport android.graphics.BitmapFactory\nimport android.graphics.drawable.Icon\nimport android.media.AudioAttributes\nimport android.media.AudioFocusRequest\nimport android.media.AudioManager\nimport android.media.MediaMetadata\nimport android.media.session.MediaSession\nimport android.media.session.PlaybackState\nimport android.os.Build\nimport android.os.Handler\nimport android.os.IBinder\nimport android.os.Looper\nimport org.json.JSONObject\nimport java.io.File\nimport java.net.HttpURLConnection\nimport java.net.URL\n\n/**\n * 잠금 화면·알림의 재생 조종과, 화면이 꺼져도 계속 트는 전경 서비스.\n *\n * 소리는 앱 안의 러스트 엔진이 낸다. 이 서비스는 엔진의 상태(SSE)를 보고 미디어 세션과 알림을\n * 맞추고, 단추(알림, 잠금 화면, 헤드셋, 블루투스)를 누르면 엔진에 명령을 보낸다. 전화가 오면\n * 멈췄다가 끝나면 다시 틀고, 이어폰이 빠지면 멈춘다.\n */\nclass SoriPlaybackService : Service() {\n  companion object {\n    const val CHANNEL = "sori.playback"\n    const val NOTE_ID = 4380\n    const val ACTION_TOGGLE = "sori.toggle"\n    const val ACTION_NEXT = "sori.next"\n    const val ACTION_PREV = "sori.prev"\n  }\n\n  private val main = Handler(Looper.getMainLooper())\n  @Volatile private var alive = true\n  private lateinit var session: MediaSession\n  private lateinit var audio: AudioManager\n  private var focus: AudioFocusRequest? = null\n  private var hasFocus = false\n  private var pausedByFocus = false\n  private var foreground = false\n  private var playing = false\n  private var status = "idle"\n  private var title = ""\n  private var artist = ""\n  private var album = ""\n  private var coverPath: String? = null\n  private var art: Bitmap? = null\n  private var durationMs = 0L\n\n  private val noisy = object : BroadcastReceiver() {\n    override fun onReceive(context: Context?, intent: Intent?) {\n      // 이어폰이 빠지면 멈춘다(스피커로 갑자기 나오지 않게).\n      if (playing) control("pause")\n    }\n  }\n\n  override fun onBind(intent: Intent?): IBinder? = null\n\n  override fun onCreate() {\n    super.onCreate()\n    audio = getSystemService(AudioManager::class.java)\n    val channel = NotificationChannel(CHANNEL, "재생", NotificationManager.IMPORTANCE_LOW)\n    channel.description = "지금 재생 중인 곡과 재생 단추"\n    channel.setShowBadge(false)\n    getSystemService(NotificationManager::class.java).createNotificationChannel(channel)\n    session = MediaSession(this, "Sori")\n    session.setCallback(object : MediaSession.Callback() {\n      override fun onPlay() {\n        control("play")\n      }\n      override fun onPause() {\n        control("pause")\n      }\n      override fun onSkipToNext() {\n        control("next")\n      }\n      override fun onSkipToPrevious() {\n        control("prev")\n      }\n      override fun onStop() {\n        control("pause")\n      }\n      override fun onSeekTo(pos: Long) {\n        control("seek", pos / 1000.0)\n      }\n    })\n    session.isActive = true\n    if (Build.VERSION.SDK_INT >= 33) {\n      registerReceiver(noisy, IntentFilter(AudioManager.ACTION_AUDIO_BECOMING_NOISY), Context.RECEIVER_NOT_EXPORTED)\n    } else {\n      registerReceiver(noisy, IntentFilter(AudioManager.ACTION_AUDIO_BECOMING_NOISY))\n    }\n    Thread({ listen() }, "sori-session").start()\n  }\n\n  override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {\n    when (intent?.action) {\n      ACTION_TOGGLE -> control(if (playing) "pause" else "play")\n      ACTION_NEXT -> control("next")\n      ACTION_PREV -> control("prev")\n    }\n    // 앱이 죽은 뒤 서비스만 되살아나면 엔진이 없다. 앱이 다시 띄운다.\n    return START_NOT_STICKY\n  }\n\n  override fun onTaskRemoved(rootIntent: Intent?) {\n    // 앱을 목록에서 밀어 닫았을 때: 틀고 있지 않으면 같이 끝낸다.\n    if (!playing) stopSelf()\n    super.onTaskRemoved(rootIntent)\n  }\n\n  override fun onDestroy() {\n    alive = false\n    try {\n      unregisterReceiver(noisy)\n    } catch (e: Exception) {\n    }\n    focus?.let { audio.abandonAudioFocusRequest(it) }\n    session.release()\n    super.onDestroy()\n  }\n\n  private fun port(): Int? =\n    try {\n      File(applicationInfo.dataDir, "server.port").readText().trim().toIntOrNull()\n    } catch (e: Exception) {\n      null\n    }\n\n  /** 엔진의 상태를 SSE로 받는다. 끊기면 다시 붙는다. */\n  private fun listen() {\n    var misses = 0\n    while (alive) {\n      // 엔진에 오래 닿지 못하고 틀고 있지도 않으면(앱이 없다) 그만둔다.\n      if (misses > 40 && !playing) {\n        main.post { stopSelf() }\n        return\n      }\n      val port = port()\n      if (port == null) {\n        misses++\n        Thread.sleep(1000)\n        continue\n      }\n      try {\n        val c = URL("http://127.0.0.1:$port/api/player/events").openConnection() as HttpURLConnection\n        c.connectTimeout = 3000\n        c.readTimeout = 40000\n        c.inputStream.bufferedReader().use { r ->\n          misses = 0\n          while (alive) {\n            val line = r.readLine() ?: break\n            if (line.startsWith("data:")) {\n              val j = JSONObject(line.substring(5).trim())\n              main.post { onState(j, port) }\n            }\n          }\n        }\n        c.disconnect()\n      } catch (e: Exception) {\n        misses++\n        try {\n          Thread.sleep(1500)\n        } catch (ie: InterruptedException) {\n        }\n      }\n    }\n  }\n\n  private fun onState(j: JSONObject, port: Int) {\n    status = j.optString("status", "idle")\n    playing = status == "playing" || status == "loading"\n    val pos = (j.optDouble("position", 0.0) * 1000).toLong()\n    durationMs = (j.optDouble("duration", 0.0) * 1000).toLong()\n    val meta = j.optJSONObject("meta")\n    val t = meta?.optString("title", "") ?: ""\n    val a = meta?.optString("artist", "") ?: ""\n    val al = meta?.optString("album", "") ?: ""\n    val cover = meta?.optString("cover", "")?.takeIf { it.isNotEmpty() }\n    val changed = t != title || a != artist || al != album\n    title = t\n    artist = a\n    album = al\n    if (cover != coverPath) {\n      coverPath = cover\n      art = null\n      if (cover != null) loadArt(port, cover)\n    }\n    if (changed) publishMeta()\n    val actions = PlaybackState.ACTION_PLAY or PlaybackState.ACTION_PAUSE or PlaybackState.ACTION_PLAY_PAUSE or\n      PlaybackState.ACTION_SKIP_TO_NEXT or PlaybackState.ACTION_SKIP_TO_PREVIOUS or PlaybackState.ACTION_SEEK_TO or\n      PlaybackState.ACTION_STOP\n    session.setPlaybackState(\n      PlaybackState.Builder()\n        .setActions(actions)\n        .setState(if (playing) PlaybackState.STATE_PLAYING else PlaybackState.STATE_PAUSED, pos, if (status == "playing") 1f else 0f)\n        .build()\n    )\n    if (playing) requestFocus()\n    updateNotification(j.isNull("uid") || status == "idle")\n  }\n\n  private fun publishMeta() {\n    val b = MediaMetadata.Builder()\n      .putString(MediaMetadata.METADATA_KEY_TITLE, title)\n      .putString(MediaMetadata.METADATA_KEY_ARTIST, artist)\n      .putString(MediaMetadata.METADATA_KEY_ALBUM, album)\n      .putLong(MediaMetadata.METADATA_KEY_DURATION, durationMs)\n    val bmp = art\n    if (bmp != null) b.putBitmap(MediaMetadata.METADATA_KEY_ALBUM_ART, bmp)\n    session.setMetadata(b.build())\n  }\n\n  private fun loadArt(port: Int, path: String) {\n    Thread {\n      val bmp: Bitmap? =\n        try {\n          val c = URL("http://127.0.0.1:$port$path").openConnection() as HttpURLConnection\n          c.connectTimeout = 3000\n          c.readTimeout = 8000\n          c.inputStream.use { BitmapFactory.decodeStream(it) }\n        } catch (e: Exception) {\n          null\n        }\n      main.post {\n        if (coverPath == path) {\n          art = bmp\n          publishMeta()\n          updateNotification(status == "idle")\n        }\n      }\n    }.start()\n  }\n\n  private fun action(icon: Int, label: String, act: String): Notification.Action {\n    val intent = Intent(this, SoriPlaybackService::class.java).setAction(act)\n    val pi = PendingIntent.getService(this, act.hashCode(), intent, PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT)\n    return Notification.Action.Builder(Icon.createWithResource(this, icon), label, pi).build()\n  }\n\n  private fun updateNotification(idle: Boolean) {\n    val nm = getSystemService(NotificationManager::class.java)\n    if (idle || title.isEmpty()) {\n      if (foreground) {\n        stopForeground(STOP_FOREGROUND_REMOVE)\n        foreground = false\n      }\n      nm.cancel(NOTE_ID)\n      return\n    }\n    val launch = packageManager.getLaunchIntentForPackage(packageName) ?: Intent(this, MainActivity::class.java)\n    val open = PendingIntent.getActivity(this, 0, launch, PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT)\n    val toggle =\n      if (playing) action(android.R.drawable.ic_media_pause, "일시정지", ACTION_TOGGLE)\n      else action(android.R.drawable.ic_media_play, "재생", ACTION_TOGGLE)\n    val builder = Notification.Builder(this, CHANNEL)\n      .setSmallIcon(R.drawable.ic_stat_sori)\n      .setContentTitle(title)\n      .setContentText(artist)\n      .setSubText(album)\n      .setContentIntent(open)\n      .setOngoing(playing)\n      .setShowWhen(false)\n      .setVisibility(Notification.VISIBILITY_PUBLIC)\n      .setCategory(Notification.CATEGORY_TRANSPORT)\n      .addAction(action(android.R.drawable.ic_media_previous, "이전", ACTION_PREV))\n      .addAction(toggle)\n      .addAction(action(android.R.drawable.ic_media_next, "다음", ACTION_NEXT))\n      .setStyle(Notification.MediaStyle().setMediaSession(session.sessionToken).setShowActionsInCompactView(0, 1, 2))\n    val bmp = art\n    if (bmp != null) builder.setLargeIcon(bmp)\n    val n = builder.build()\n    if (playing) {\n      try {\n        if (Build.VERSION.SDK_INT >= 29) {\n          startForeground(NOTE_ID, n, ServiceInfo.FOREGROUND_SERVICE_TYPE_MEDIA_PLAYBACK)\n        } else {\n          startForeground(NOTE_ID, n)\n        }\n        foreground = true\n      } catch (e: Exception) {\n        nm.notify(NOTE_ID, n)\n      }\n    } else {\n      // 멈춰 있으면 밀어서 지울 수 있게 둔다(다시 틀면 앞으로 나온다).\n      if (foreground) {\n        stopForeground(STOP_FOREGROUND_DETACH)\n        foreground = false\n      }\n      nm.notify(NOTE_ID, n)\n    }\n  }\n\n  private fun requestFocus() {\n    if (hasFocus) return\n    var req = focus\n    if (req == null) {\n      val attrs = AudioAttributes.Builder()\n        .setUsage(AudioAttributes.USAGE_MEDIA)\n        .setContentType(AudioAttributes.CONTENT_TYPE_MUSIC)\n        .build()\n      req = AudioFocusRequest.Builder(AudioManager.AUDIOFOCUS_GAIN)\n        .setAudioAttributes(attrs)\n        .setWillPauseWhenDucked(false)\n        .setOnAudioFocusChangeListener(AudioManager.OnAudioFocusChangeListener { change -> onFocus(change) }, main)\n        .build()\n      focus = req\n    }\n    hasFocus = audio.requestAudioFocus(req!!) == AudioManager.AUDIOFOCUS_REQUEST_GRANTED\n  }\n\n  private fun onFocus(change: Int) {\n    when (change) {\n      AudioManager.AUDIOFOCUS_LOSS -> {\n        // 다른 앱이 틀기 시작했다.\n        hasFocus = false\n        pausedByFocus = false\n        control("pause")\n      }\n      AudioManager.AUDIOFOCUS_LOSS_TRANSIENT -> {\n        // 전화, 길 안내: 잠깐 멈췄다가 돌려받으면 다시 튼다.\n        hasFocus = false\n        if (playing) {\n          pausedByFocus = true\n          control("pause")\n        }\n      }\n      AudioManager.AUDIOFOCUS_GAIN -> {\n        hasFocus = true\n        if (pausedByFocus) {\n          pausedByFocus = false\n          control("play")\n        }\n      }\n    }\n  }\n\n  private fun control(cmd: String, t: Double = 0.0) {\n    val port = port() ?: return\n    Thread {\n      try {\n        val c = URL("http://127.0.0.1:$port/api/player/control").openConnection() as HttpURLConnection\n        c.requestMethod = "POST"\n        c.doOutput = true\n        c.connectTimeout = 2000\n        c.readTimeout = 4000\n        c.setRequestProperty("Content-Type", "application/json")\n        c.outputStream.use { it.write("{\\"cmd\\":\\"$cmd\\",\\"t\\":$t}".toByteArray()) }\n        c.inputStream.close()\n        c.disconnect()\n      } catch (e: Exception) {\n      }\n    }.start()\n  }\n}\n'

root = pathlib.Path(sys.argv[1] if len(sys.argv) > 1 else 'crates/sori-app/gen/android')

UPDATE_PLUGIN = '''package __PKG__

import android.app.Activity
import android.content.Intent
import android.net.Uri
import android.os.Build
import android.provider.Settings
import androidx.core.content.FileProvider
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.Plugin
import java.io.File

@InvokeArg
class InstallArgs {
  lateinit var path: String
}

@InvokeArg
class OpenArgs {
  lateinit var url: String
}

/**
 * 자동 업데이트의 마지막 단계. 소리 서버(러스트)가 받아 둔 새 APK를 시스템 설치 창으로 연다.
 * 안드로이드는 앱이 스스로 설치할 수 없으므로 사용자가 설치를 누른다(같은 서명이라 덮어 설치된다).
 */
@TauriPlugin
class UpdatePlugin(private val activity: Activity) : Plugin(activity) {
  @Command
  fun install(invoke: Invoke) {
    try {
      val args = invoke.parseArgs(InstallArgs::class.java)
      val file = File(args.path)
      if (!file.isFile) {
        invoke.reject("받아 둔 파일이 없습니다")
        return
      }
      if (Build.VERSION.SDK_INT >= 26 && !activity.packageManager.canRequestPackageInstalls()) {
        // 처음 한 번: 이 앱이 설치 창을 띄워도 된다고 허락하는 화면으로 보낸다.
        activity.startActivity(Intent(Settings.ACTION_MANAGE_UNKNOWN_APP_SOURCES, Uri.parse("package:" + activity.packageName)))
        invoke.reject("설정에서 소리의 앱 설치를 허용한 뒤 다시 눌러 주세요")
        return
      }
      val uri = FileProvider.getUriForFile(activity, activity.packageName + ".update", file)
      val intent = Intent(Intent.ACTION_VIEW)
        .setDataAndType(uri, "application/vnd.android.package-archive")
        .addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION or Intent.FLAG_ACTIVITY_NEW_TASK)
      activity.startActivity(intent)
      invoke.resolve()
    } catch (e: Exception) {
      invoke.reject(e.message ?: "설치 창을 열지 못했습니다")
    }
  }

  /** 주소를 앱 밖에서 연다(기본 브라우저, 유튜브 앱). 앱의 창이 그 주소로 넘어가지 않게. */
  @Command
  fun open(invoke: Invoke) {
    try {
      val uri = Uri.parse(invoke.parseArgs(OpenArgs::class.java).url)
      if (uri.scheme != "https" && uri.scheme != "http") {
        invoke.reject("열 수 없는 주소입니다")
        return
      }
      activity.startActivity(Intent(Intent.ACTION_VIEW, uri).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
      invoke.resolve()
    } catch (e: Exception) {
      invoke.reject(e.message ?: "열지 못했습니다")
    }
  }
}

/** 설치 창이 받아 둔 APK를 읽게 해 주는 곳. 다른 FileProvider와 겹치지 않게 따로 둔다. */
class UpdateFileProvider : FileProvider()
'''

manifest = root / 'app/src/main/AndroidManifest.xml'
m = manifest.read_text()
perms = [
    '<uses-permission android:name="android.permission.READ_MEDIA_AUDIO" />',
    '<uses-permission android:name="android.permission.READ_MEDIA_IMAGES" />',
    '<uses-permission android:name="android.permission.READ_EXTERNAL_STORAGE" android:maxSdkVersion="32" />',
    # 라이브 아카이브 공연을 음악 폴더에 저장한다. 안드로이드 11부터는 권한 없이 Music 폴더에 음악 파일을
    # 만들 수 있고, 10 이하에서만 이 권한이 필요하다.
    '<uses-permission android:name="android.permission.WRITE_EXTERNAL_STORAGE" android:maxSdkVersion="29" />',
    '<uses-permission android:name="android.permission.WAKE_LOCK" />',
    # 화면이 꺼져도 계속 틀고, 잠금 화면·알림에서 조종한다.
    '<uses-permission android:name="android.permission.FOREGROUND_SERVICE" />',
    '<uses-permission android:name="android.permission.FOREGROUND_SERVICE_MEDIA_PLAYBACK" />',
    '<uses-permission android:name="android.permission.POST_NOTIFICATIONS" />',
    # 자동 업데이트: 받아 둔 새 버전의 설치 창을 띄운다(설치는 사용자가 누른다).
    '<uses-permission android:name="android.permission.REQUEST_INSTALL_PACKAGES" />',
]
missing = [p for p in perms if p.split('"')[1] not in m]
if missing:
    m = m.replace('<application', '\n    '.join(missing) + '\n\n    <application', 1)
if 'SoriPlaybackService' not in m:
    m = m.replace('</application>', '''    <service
            android:name=".SoriPlaybackService"
            android:exported="false"
            android:foregroundServiceType="mediaPlayback" />
    </application>''', 1)
if 'UpdateFileProvider' not in m:
    m = m.replace('</application>', '''    <provider
            android:name=".UpdateFileProvider"
            android:authorities="${applicationId}.update"
            android:exported="false"
            android:grantUriPermissions="true">
            <meta-data
                android:name="android.support.FILE_PROVIDER_PATHS"
                android:resource="@xml/sori_update_paths" />
        </provider>
    </application>''', 1)
# 안드로이드 10: 음악 폴더에 직접 쓰려면 옛 저장공간 방식을 쓴다(11부터는 무시된다).
if 'requestLegacyExternalStorage' not in m:
    m = m.replace('<application', '<application android:requestLegacyExternalStorage="true"', 1)
# 평문 주소(127.0.0.1)를 릴리스에서도 열 수 있게
m = re.sub(r'android:usesCleartextTraffic="[^"]*"', 'android:usesCleartextTraffic="true"', m)
if 'usesCleartextTraffic' not in m:
    m = m.replace('<application', '<application android:usesCleartextTraffic="true"', 1)
manifest.write_text(m)

gradle = root / 'app/build.gradle.kts'
g = gradle.read_text()
g = re.sub(r'manifestPlaceholders\["usesCleartextTraffic"\]\s*=\s*"false"', 'manifestPlaceholders["usesCleartextTraffic"] = "true"', g)
gradle.write_text(g)

acts = list(root.glob('app/src/main/java/**/MainActivity.kt'))
if not acts:
    sys.exit('MainActivity.kt를 찾지 못했습니다')
act = acts[0]
pkg = re.search(r'^package\s+([\w.]+)', act.read_text(), re.M).group(1)
act.write_text(f'''package {pkg}

import android.Manifest
import android.content.pm.ActivityInfo
import android.content.pm.PackageManager
import android.graphics.Bitmap
import android.graphics.Color
import android.net.Uri
import android.os.Build
import android.os.Bundle
import android.view.View
import android.view.ViewGroup
import android.webkit.ConsoleMessage
import android.webkit.GeolocationPermissions
import android.webkit.JsPromptResult
import android.webkit.JsResult
import android.webkit.PermissionRequest
import android.webkit.ValueCallback
import android.webkit.WebChromeClient
import android.webkit.WebView
import android.widget.FrameLayout
import androidx.activity.OnBackPressedCallback
import androidx.activity.enableEdgeToEdge
import androidx.core.view.ViewCompat
import androidx.core.view.WindowCompat
import androidx.core.view.WindowInsetsCompat
import androidx.core.view.WindowInsetsControllerCompat

class MainActivity : TauriActivity() {{
  // 웹뷰 안의 전체 화면(유튜브 플레이어의 전체 화면 단추). wry의 클라이언트는 전체 화면을 곧바로 닫아
  // 버리므로, 그 위에 전체 화면만 맡는 껍질을 씌우고 나머지는 그대로 넘긴다.
  private var fullscreen: View? = null
  private var fullscreenDone: WebChromeClient.CustomViewCallback? = null
  private val fullscreenBack = object : OnBackPressedCallback(false) {{
    override fun handleOnBackPressed() {{
      exitFullscreen()
    }}
  }}

  override fun onWebViewCreate(webView: WebView) {{
    // 앱 화면은 확대하지 않는다(웹 페이지처럼 두 손가락으로 벌리거나 두 번 눌러 커지지 않게).
    webView.settings.setSupportZoom(false)
    webView.settings.builtInZoomControls = false
    webView.settings.displayZoomControls = false
    // wry는 이 함수를 부른 다음에 클라이언트를 붙인다. 한 박자 뒤에 감싼다.
    webView.post {{
      val inner = webView.webChromeClient ?: return@post
      webView.webChromeClient = FullscreenChromeClient(inner)
      onBackPressedDispatcher.addCallback(this, fullscreenBack)
    }}
  }}

  private fun enterFullscreen(view: View, done: WebChromeClient.CustomViewCallback) {{
    if (fullscreen != null) {{
      done.onCustomViewHidden()
      return
    }}
    fullscreen = view
    fullscreenDone = done
    view.setBackgroundColor(Color.BLACK)
    (window.decorView as FrameLayout).addView(view, FrameLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.MATCH_PARENT))
    WindowCompat.getInsetsController(window, window.decorView).apply {{
      hide(WindowInsetsCompat.Type.systemBars())
      systemBarsBehavior = WindowInsetsControllerCompat.BEHAVIOR_SHOW_TRANSIENT_BARS_BY_SWIPE
    }}
    requestedOrientation = ActivityInfo.SCREEN_ORIENTATION_SENSOR_LANDSCAPE
    fullscreenBack.isEnabled = true
  }}

  private fun exitFullscreen() {{
    val view = fullscreen ?: return
    (window.decorView as FrameLayout).removeView(view)
    fullscreen = null
    WindowCompat.getInsetsController(window, window.decorView).show(WindowInsetsCompat.Type.systemBars())
    requestedOrientation = ActivityInfo.SCREEN_ORIENTATION_UNSPECIFIED
    fullscreenBack.isEnabled = false
    fullscreenDone?.onCustomViewHidden()
    fullscreenDone = null
  }}

  inner class FullscreenChromeClient(private val inner: WebChromeClient) : WebChromeClient() {{
    override fun onShowCustomView(view: View, callback: CustomViewCallback) = enterFullscreen(view, callback)
    override fun onHideCustomView() = exitFullscreen()
    override fun onPermissionRequest(request: PermissionRequest) = inner.onPermissionRequest(request)
    override fun onPermissionRequestCanceled(request: PermissionRequest) = inner.onPermissionRequestCanceled(request)
    override fun getDefaultVideoPoster(): Bitmap? = inner.defaultVideoPoster
    override fun onJsAlert(view: WebView, url: String, message: String, result: JsResult): Boolean = inner.onJsAlert(view, url, message, result)
    override fun onJsConfirm(view: WebView, url: String, message: String, result: JsResult): Boolean = inner.onJsConfirm(view, url, message, result)
    override fun onJsPrompt(view: WebView, url: String, message: String, defaultValue: String?, result: JsPromptResult): Boolean =
      inner.onJsPrompt(view, url, message, defaultValue, result)
    override fun onGeolocationPermissionsShowPrompt(origin: String, callback: GeolocationPermissions.Callback) =
      inner.onGeolocationPermissionsShowPrompt(origin, callback)
    override fun onGeolocationPermissionsHidePrompt() = inner.onGeolocationPermissionsHidePrompt()
    override fun onShowFileChooser(view: WebView, filePathCallback: ValueCallback<Array<Uri>>, fileChooserParams: FileChooserParams): Boolean =
      inner.onShowFileChooser(view, filePathCallback, fileChooserParams)
    override fun onConsoleMessage(consoleMessage: ConsoleMessage): Boolean = inner.onConsoleMessage(consoleMessage)
    override fun onReceivedTitle(view: WebView, title: String?) = inner.onReceivedTitle(view, title)
    override fun onProgressChanged(view: WebView, newProgress: Int) = inner.onProgressChanged(view, newProgress)
  }}

  override fun onCreate(savedInstanceState: Bundle?) {{
    enableEdgeToEdge()
    super.onCreate(savedInstanceState)
    // 상태 표시줄·제스처 막대·키보드 자리만큼 화면을 띄운다. 그 자리는 앱의 바탕색, 아이콘은 밝게.
    window.decorView.setBackgroundColor(Color.parseColor("#0E0E10"))
    WindowCompat.getInsetsController(window, window.decorView).apply {{
      isAppearanceLightStatusBars = false
      isAppearanceLightNavigationBars = false
    }}
    val content = findViewById<android.view.View>(android.R.id.content)
    ViewCompat.setOnApplyWindowInsetsListener(content) {{ v, insets ->
      val bars = insets.getInsets(WindowInsetsCompat.Type.systemBars() or WindowInsetsCompat.Type.displayCutout())
      val ime = insets.getInsets(WindowInsetsCompat.Type.ime())
      v.setPadding(bars.left, bars.top, bars.right, maxOf(bars.bottom, ime.bottom))
      WindowInsetsCompat.CONSUMED
    }}
    // 잠금 화면·알림의 재생 조종(엔진이 틀기 시작하면 앞으로 나온다).
    try {{
      startService(android.content.Intent(this, SoriPlaybackService::class.java))
    }} catch (e: Exception) {{
    }}
    // 음악 폴더를 읽으려면 사용자의 허락이 필요하다. 허락하면 소리가 몇 초 안에 다시 스캔한다.
    // 안드로이드 10 이하는 라이브 공연을 음악 폴더에 저장할 권한도 함께 묻는다(같은 창 하나).
    val wanted = if (Build.VERSION.SDK_INT >= 33)
      arrayOf(Manifest.permission.READ_MEDIA_AUDIO, Manifest.permission.READ_MEDIA_IMAGES, Manifest.permission.POST_NOTIFICATIONS)
    else if (Build.VERSION.SDK_INT <= 29)
      arrayOf(Manifest.permission.READ_EXTERNAL_STORAGE, Manifest.permission.WRITE_EXTERNAL_STORAGE)
    else
      arrayOf(Manifest.permission.READ_EXTERNAL_STORAGE)
    val missing = wanted.filter {{ checkSelfPermission(it) != PackageManager.PERMISSION_GRANTED }}
    if (missing.isNotEmpty()) requestPermissions(missing.toTypedArray(), 1)
  }}

  override fun onResume() {{
    super.onResume()
    // 앱이 뒤에 오래 있으면 시스템이 (틀지 않는) 서비스를 거둔다. 앞에 나올 때마다 다시 띄운다.
    try {{
      startService(android.content.Intent(this, SoriPlaybackService::class.java))
    }} catch (e: Exception) {{
    }}
  }}
}}
''')

service = act.parent / 'SoriPlaybackService.kt'
service.write_text(SERVICE.replace('__PKG__', pkg))
(act.parent / 'UpdatePlugin.kt').write_text(UPDATE_PLUGIN.replace('__PKG__', pkg))
paths = root / 'app/src/main/res/xml/sori_update_paths.xml'
paths.parent.mkdir(parents=True, exist_ok=True)
# 소리 서버는 새 APK를 앱의 cache/update 아래에 받아 둔다.
paths.write_text('<?xml version="1.0" encoding="utf-8"?>\n<paths>\n    <cache-path name="update" path="update/" />\n</paths>\n')
icon = root / 'app/src/main/res/drawable/ic_stat_sori.xml'
icon.parent.mkdir(parents=True, exist_ok=True)
icon.write_text('''<vector xmlns:android="http://schemas.android.com/apk/res/android"
    android:width="24dp" android:height="24dp" android:viewportWidth="24" android:viewportHeight="24">
    <path android:fillColor="#FFFFFFFF" android:pathData="M4,9h2.4v6h-2.4z M8.8,5h2.4v14h-2.4z M13.6,7h2.4v10h-2.4z M18.4,10h2.4v4h-2.4z" />
</vector>
''')

# 테마: 처음 뜨는 순간(스플래시)부터 앱의 어두운 바탕
colors = root / 'app/src/main/res/values/colors.xml'
if colors.exists():
    c = colors.read_text()
    if 'sori_bg' not in c:
        c = c.replace('</resources>', '    <color name="sori_bg">#0E0E10</color>\n</resources>')
else:
    c = '<?xml version="1.0" encoding="utf-8"?>\n<resources>\n    <color name="sori_bg">#0E0E10</color>\n</resources>\n'
colors.write_text(c)
for t in root.glob('app/src/main/res/values*/themes.xml'):
    x = t.read_text()
    if 'sori_bg' not in x:
        x = x.replace('<!-- Customize your theme here. -->', '''<item name="android:windowBackground">@color/sori_bg</item>
        <item name="android:statusBarColor">@color/sori_bg</item>
        <item name="android:navigationBarColor">@color/sori_bg</item>
        <item name="android:windowLightStatusBar">false</item>
        <item name="android:windowLightNavigationBar" tools:targetApi="27">false</item>''')
        t.write_text(x)

print('patched', manifest, gradle, act, sep='\n  ')
print(manifest.read_text())
