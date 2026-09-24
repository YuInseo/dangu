package com.dangu.gallery.data

import android.content.Context
import android.content.Intent
import android.net.Uri
import android.provider.Settings
import androidx.core.content.FileProvider
import com.dangu.gallery.BuildConfig
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.withContext
import org.json.JSONObject
import java.io.File
import java.net.HttpURLConnection
import java.net.URL

/**
 * 앱을 켤 때 새 버전을 확인하고, 있으면 APK를 미리 받아 둔다.
 *
 * CI가 `gallery-latest` 릴리스에 APK와 `gallery-version.json`을 올린다. 안드로이드는
 * 사이드로딩한 앱이 스스로를 조용히 설치하도록 두지 않으므로, 설치는 시스템 확인 창
 * 한 번을 거친다 — 그 전까지(확인·다운로드)는 전부 알아서 한다.
 */
class Updater(context: Context) {
    private val appContext = context.applicationContext
    private val dir = File(appContext.cacheDir, "update")

    sealed interface State {
        data object Idle : State
        data object Checking : State
        data object UpToDate : State
        data class Downloading(val versionName: String, val progress: Float) : State
        data class Ready(val versionName: String, val apk: File) : State
        data class Failed(val message: String) : State
    }

    private val _state = MutableStateFlow<State>(State.Idle)
    val state: StateFlow<State> = _state.asStateFlow()

    val currentVersion: String get() = BuildConfig.VERSION_NAME

    private val base = "https://github.com/${BuildConfig.UPDATE_REPO}/releases/download/gallery-latest/"

    suspend fun check() = withContext(Dispatchers.IO) {
        if (BuildConfig.UPDATE_REPO.isBlank()) return@withContext
        val s = _state.value
        if (s is State.Checking || s is State.Downloading) return@withContext
        _state.value = State.Checking
        try {
            val json = JSONObject(fetchText(base + "gallery-version.json"))
            val code = json.getInt("versionCode")
            val name = json.optString("versionName", code.toString())
            if (code <= BuildConfig.VERSION_CODE) {
                dir.deleteRecursively()
                _state.value = State.UpToDate
                return@withContext
            }
            val apk = File(dir, "gallery-view-$code.apk")
            if (!apk.exists()) {
                dir.deleteRecursively()
                dir.mkdirs()
                download(base + (json.optString("apk").ifBlank { "gallery-view.apk" }), apk) { p ->
                    _state.value = State.Downloading(name, p)
                }
            }
            _state.value = State.Ready(name, apk)
        } catch (e: Exception) {
            _state.value = State.Failed(e.message ?: e.javaClass.simpleName)
        }
    }

    /** 받아 둔 APK의 설치 창을 띄운다. 이 앱의 "출처를 알 수 없는 앱" 허용이 없으면 그 설정부터. */
    fun install(context: Context) {
        val ready = _state.value as? State.Ready ?: return
        val pm = context.packageManager
        if (!pm.canRequestPackageInstalls()) {
            context.startActivity(
                Intent(Settings.ACTION_MANAGE_UNKNOWN_APP_SOURCES, Uri.parse("package:${context.packageName}"))
                    .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
            )
            return
        }
        val uri = FileProvider.getUriForFile(context, "${context.packageName}.files", ready.apk)
        context.startActivity(
            Intent(Intent.ACTION_VIEW)
                .setDataAndType(uri, "application/vnd.android.package-archive")
                .addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION or Intent.FLAG_ACTIVITY_NEW_TASK)
        )
    }

    fun dismiss() {
        if (_state.value !is State.Downloading) _state.value = State.Idle
    }

    private fun open(url: String): HttpURLConnection {
        var target = url
        // GitHub 릴리스 파일은 다른 호스트로 넘겨주므로 따라간다.
        repeat(5) {
            val conn = (URL(target).openConnection() as HttpURLConnection).apply {
                connectTimeout = 15_000
                readTimeout = 30_000
                instanceFollowRedirects = false
                setRequestProperty("User-Agent", "gallery-view/${BuildConfig.VERSION_NAME}")
            }
            val code = conn.responseCode
            if (code in 300..399) {
                target = conn.getHeaderField("Location") ?: throw IllegalStateException("리다이렉트 주소 없음")
                conn.disconnect()
            } else if (code != 200) {
                conn.disconnect()
                throw IllegalStateException("HTTP $code")
            } else {
                return conn
            }
        }
        throw IllegalStateException("리다이렉트가 너무 많음")
    }

    private fun fetchText(url: String): String {
        val conn = open(url)
        return try { conn.inputStream.bufferedReader().readText() } finally { conn.disconnect() }
    }

    private fun download(url: String, dest: File, onProgress: (Float) -> Unit) {
        val conn = open(url)
        val tmp = File(dest.path + ".part")
        try {
            val total = conn.contentLengthLong
            var read = 0L
            var lastReport = 0L
            conn.inputStream.use { input ->
                tmp.outputStream().use { out ->
                    val buf = ByteArray(64 * 1024)
                    while (true) {
                        val n = input.read(buf)
                        if (n < 0) break
                        out.write(buf, 0, n)
                        read += n
                        if (total > 0 && read - lastReport > 256 * 1024) {
                            lastReport = read
                            onProgress(read / total.toFloat())
                        }
                    }
                }
            }
            if (total > 0 && read != total) throw IllegalStateException("다운로드가 중간에 끊김")
            if (!tmp.renameTo(dest)) throw IllegalStateException("파일을 옮기지 못함")
        } finally {
            conn.disconnect()
            tmp.delete()
        }
    }

    companion object {
        @Volatile private var instance: Updater? = null
        fun get(context: Context): Updater =
            instance ?: synchronized(this) {
                instance ?: Updater(context).also { instance = it }
            }
    }
}
