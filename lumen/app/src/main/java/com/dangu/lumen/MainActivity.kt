package com.dangu.lumen

import android.Manifest
import android.annotation.SuppressLint
import android.app.DownloadManager
import android.content.ClipData
import android.content.ClipboardManager
import android.content.Intent
import android.content.pm.PackageManager
import android.graphics.Color
import android.net.Uri
import android.os.Build
import android.os.Bundle
import android.os.Environment
import android.webkit.CookieManager
import android.webkit.PermissionRequest
import android.webkit.URLUtil
import android.webkit.ValueCallback
import android.webkit.WebChromeClient
import android.webkit.WebResourceRequest
import android.webkit.WebSettings
import android.webkit.WebView
import android.webkit.WebViewClient
import android.widget.Toast
import androidx.activity.ComponentActivity
import androidx.activity.OnBackPressedCallback
import androidx.activity.SystemBarStyle
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.activity.result.contract.ActivityResultContracts
import androidx.core.content.ContextCompat
import androidx.core.content.IntentCompat
import androidx.lifecycle.lifecycleScope
import androidx.webkit.ScriptHandler
import androidx.webkit.WebViewCompat
import androidx.webkit.WebViewFeature
import com.dangu.lumen.ui.LumenScreen
import kotlinx.coroutines.flow.collectLatest
import kotlinx.coroutines.launch

class MainActivity : ComponentActivity() {
    lateinit var prefs: Prefs
    lateinit var webView: WebView
    private var startScript: ScriptHandler? = null
    @Volatile private var currentHost: String? = null

    /** 다른 앱에서 공유해 온 파일. 다음 "파일 첨부" 때 고르는 창 없이 바로 들어간다. */
    private var pendingShare: List<Uri> = emptyList()
    private var fileCallback: ValueCallback<Array<Uri>>? = null
    private var pendingPermission: PermissionRequest? = null

    private val fileChooser = registerForActivityResult(ActivityResultContracts.StartActivityForResult()) { result ->
        val data = result.data
        val uris = mutableListOf<Uri>()
        if (result.resultCode == RESULT_OK && data != null) {
            val clip = data.clipData
            if (clip != null) for (i in 0 until clip.itemCount) uris += clip.getItemAt(i).uri
            else data.data?.let { uris += it }
        }
        fileCallback?.onReceiveValue(uris.toTypedArray())
        fileCallback = null
    }

    private val mediaPermissions = registerForActivityResult(ActivityResultContracts.RequestMultiplePermissions()) {
        val request = pendingPermission ?: return@registerForActivityResult
        pendingPermission = null
        grantWhatWeCan(request)
    }

    val notificationPermission = registerForActivityResult(ActivityResultContracts.RequestPermission()) { }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        prefs = Prefs(this)
        applySystemBars()
        webView = createWebView()

        installStartScript() // 첫 페이지보다 먼저
        if (savedInstanceState != null) webView.restoreState(savedInstanceState)
        if (webView.url == null) webView.loadUrl(urlFrom(intent) ?: HOME)
        handleShare(intent)

        onBackPressedDispatcher.addCallback(this, object : OnBackPressedCallback(true) {
            override fun handleOnBackPressed() {
                // 디스코드 안에서 뒤로 갈 곳이 있으면 거기로, 없으면 앱을 끄지 않고 뒤로 보낸다
                // (연결이 살아 있어야 알림이 온다).
                if (webView.canGoBack()) webView.goBack() else moveTaskToBack(true)
            }
        })

        lifecycleScope.launch {
            prefs.state.collectLatest { s ->
                webView.settings.textZoom = s.textZoom
                installStartScript()
                val css = Themes.css(Themes.byId(s.themeId), s)
                webView.evaluateJavascript(Injector.applyCall(css, s.wideLayout), null)
                applySystemBars()
            }
        }

        if (prefs.value.notifications && Build.VERSION.SDK_INT >= 33 &&
            ContextCompat.checkSelfPermission(this, Manifest.permission.POST_NOTIFICATIONS) != PackageManager.PERMISSION_GRANTED
        ) {
            notificationPermission.launch(Manifest.permission.POST_NOTIFICATIONS)
        }

        setContent { LumenScreen(this) }
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        urlFrom(intent)?.let { webView.loadUrl(it) }
        handleShare(intent)
    }

    override fun onSaveInstanceState(outState: Bundle) {
        super.onSaveInstanceState(outState)
        webView.saveState(outState)
    }

    override fun onDestroy() {
        webView.destroy()
        super.onDestroy()
    }

    private fun applySystemBars() {
        val theme = Themes.byId(prefs.value.themeId)
        val bar = if (theme.id == "original") 0xFF1E1F22.toInt() else theme.base.toInt()
        enableEdgeToEdge(
            statusBarStyle = if (theme.light) SystemBarStyle.light(bar, bar) else SystemBarStyle.dark(bar),
            navigationBarStyle = if (theme.light) SystemBarStyle.light(bar, bar) else SystemBarStyle.dark(bar),
        )
    }

    fun reload() = webView.reload()

    fun goHome() = webView.loadUrl(HOME)

    /** 테마·알림 설정이 들어간 문서 시작 스크립트를 (다시) 건다. */
    private fun installStartScript() {
        val s = prefs.value
        val css = Themes.css(Themes.byId(s.themeId), s)
        val script = Injector.documentStart(css, s.wideLayout, s.notifications)
        if (WebViewFeature.isFeatureSupported(WebViewFeature.DOCUMENT_START_SCRIPT)) {
            startScript?.remove()
            startScript = WebViewCompat.addDocumentStartJavaScript(webView, script, ALLOWED_ORIGINS)
        }
        lastScript = script
    }

    private var lastScript: String = ""

    @SuppressLint("SetJavaScriptEnabled")
    private fun createWebView(): WebView {
        val wv = WebView(this)
        wv.setBackgroundColor(Color.TRANSPARENT)
        wv.settings.apply {
            javaScriptEnabled = true
            domStorageEnabled = true
            databaseEnabled = true
            mediaPlaybackRequiresUserGesture = false
            loadWithOverviewMode = true
            useWideViewPort = true
            setSupportZoom(false)
            builtInZoomControls = false
            javaScriptCanOpenWindowsAutomatically = false
            setSupportMultipleWindows(false)
            mixedContentMode = WebSettings.MIXED_CONTENT_NEVER_ALLOW
            allowFileAccess = false
            allowContentAccess = true
            // 디스코드는 휴대폰 브라우저에는 "앱을 받으세요" 화면을 띄운다. 데스크톱 크롬으로 보인다.
            userAgentString = DESKTOP_UA
            textZoom = prefs.value.textZoom
        }
        CookieManager.getInstance().apply {
            setAcceptCookie(true)
            setAcceptThirdPartyCookies(wv, true) // 로그인 캡차(hCaptcha)
        }
        wv.addJavascriptInterface(LumenBridge(this) { isDiscord(currentHost) }, "LumenBridge")

        wv.webViewClient = object : WebViewClient() {
            override fun shouldOverrideUrlLoading(view: WebView, request: WebResourceRequest): Boolean {
                val uri = request.url
                val host = uri.host
                if (uri.scheme == "https" && isDiscord(host)) {
                    if (host == "discord.gg") {
                        view.loadUrl("https://discord.com/invite" + (uri.path ?: ""))
                        return true
                    }
                    return false
                }
                // 로그인 창 안의 캡차·패스키 같은 건 디스코드가 알아서 띄우는 하위 프레임이라 여기 오지 않는다.
                // 여기 오는 건 사용자가 누른 바깥 링크 — 브라우저로.
                if (request.isForMainFrame || request.hasGesture()) {
                    openExternal(uri)
                    return true
                }
                return false
            }

            override fun onPageStarted(view: WebView, url: String?, favicon: android.graphics.Bitmap?) {
                currentHost = url?.let { Uri.parse(it).host }
                // 문서 시작 스크립트를 못 쓰는 오래된 WebView면 여기서라도 넣는다.
                if (!WebViewFeature.isFeatureSupported(WebViewFeature.DOCUMENT_START_SCRIPT) && isDiscord(currentHost)) {
                    view.evaluateJavascript(lastScript, null)
                }
            }

            override fun onPageFinished(view: WebView, url: String?) {
                currentHost = url?.let { Uri.parse(it).host }
                if (isDiscord(currentHost)) {
                    if (!WebViewFeature.isFeatureSupported(WebViewFeature.DOCUMENT_START_SCRIPT)) {
                        view.evaluateJavascript(lastScript, null)
                    }
                    CookieManager.getInstance().flush()
                }
            }

            override fun doUpdateVisitedHistory(view: WebView, url: String?, isReload: Boolean) {
                currentHost = url?.let { Uri.parse(it).host }
            }
        }

        wv.webChromeClient = object : WebChromeClient() {
            override fun onShowFileChooser(
                view: WebView,
                callback: ValueCallback<Array<Uri>>,
                params: FileChooserParams,
            ): Boolean {
                fileCallback?.onReceiveValue(null)
                if (pendingShare.isNotEmpty()) {
                    callback.onReceiveValue(pendingShare.toTypedArray())
                    pendingShare = emptyList()
                    return true
                }
                fileCallback = callback
                val pick = Intent(Intent.ACTION_GET_CONTENT)
                    .addCategory(Intent.CATEGORY_OPENABLE)
                    .setType("*/*")
                    .putExtra(Intent.EXTRA_ALLOW_MULTIPLE, params.mode == FileChooserParams.MODE_OPEN_MULTIPLE)
                val types = params.acceptTypes.filter { it.isNotBlank() }.flatMap { it.split(',') }.map { it.trim() }
                    .filter { it.contains('/') }
                if (types.isNotEmpty()) pick.putExtra(Intent.EXTRA_MIME_TYPES, types.toTypedArray())
                return runCatching { fileChooser.launch(Intent.createChooser(pick, "파일 첨부")); true }
                    .getOrElse { fileCallback = null; false }
            }

            override fun onPermissionRequest(request: PermissionRequest) {
                if (!isDiscord(request.origin.host)) {
                    request.deny()
                    return
                }
                val needed = request.resources.mapNotNull {
                    when (it) {
                        PermissionRequest.RESOURCE_AUDIO_CAPTURE -> Manifest.permission.RECORD_AUDIO
                        PermissionRequest.RESOURCE_VIDEO_CAPTURE -> Manifest.permission.CAMERA
                        else -> null
                    }
                }.filter {
                    ContextCompat.checkSelfPermission(this@MainActivity, it) != PackageManager.PERMISSION_GRANTED
                }
                if (needed.isEmpty()) grantWhatWeCan(request)
                else {
                    pendingPermission?.deny()
                    pendingPermission = request
                    mediaPermissions.launch(needed.toTypedArray())
                }
            }
        }

        wv.setDownloadListener { url, userAgent, contentDisposition, mimeType, _ ->
            runCatching {
                val name = URLUtil.guessFileName(url, contentDisposition, mimeType)
                val req = DownloadManager.Request(Uri.parse(url))
                    .setMimeType(mimeType)
                    .addRequestHeader("User-Agent", userAgent)
                    .setTitle(name)
                    .setNotificationVisibility(DownloadManager.Request.VISIBILITY_VISIBLE_NOTIFY_COMPLETED)
                    .setDestinationInExternalPublicDir(Environment.DIRECTORY_DOWNLOADS, "Lumen/$name")
                CookieManager.getInstance().getCookie(url)?.let { req.addRequestHeader("Cookie", it) }
                getSystemService(DownloadManager::class.java).enqueue(req)
                Toast.makeText(this, "다운로드: $name", Toast.LENGTH_SHORT).show()
            }.onFailure { openExternal(Uri.parse(url)) }
        }
        return wv
    }

    private fun grantWhatWeCan(request: PermissionRequest) {
        val ok = request.resources.filter {
            when (it) {
                PermissionRequest.RESOURCE_AUDIO_CAPTURE ->
                    ContextCompat.checkSelfPermission(this, Manifest.permission.RECORD_AUDIO) == PackageManager.PERMISSION_GRANTED
                PermissionRequest.RESOURCE_VIDEO_CAPTURE ->
                    ContextCompat.checkSelfPermission(this, Manifest.permission.CAMERA) == PackageManager.PERMISSION_GRANTED
                else -> false
            }
        }
        if (ok.isEmpty()) request.deny() else request.grant(ok.toTypedArray())
    }

    private fun openExternal(uri: Uri) {
        runCatching {
            startActivity(Intent(Intent.ACTION_VIEW, uri).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
        }
    }

    private fun urlFrom(intent: Intent?): String? {
        if (intent?.action != Intent.ACTION_VIEW) return null
        val uri = intent.data ?: return null
        if (!isDiscord(uri.host)) return null
        return if (uri.host == "discord.gg") "https://discord.com/invite" + (uri.path ?: "") else uri.toString()
    }

    /** 공유로 받은 것: 글은 클립보드로, 파일은 다음 첨부 때 바로. */
    private fun handleShare(intent: Intent?) {
        if (intent?.action != Intent.ACTION_SEND) return
        val stream = IntentCompat.getParcelableExtra(intent, Intent.EXTRA_STREAM, Uri::class.java)
        val text = intent.getStringExtra(Intent.EXTRA_TEXT)
        if (stream != null) {
            pendingShare = listOf(stream)
            Toast.makeText(this, "채널을 고르고 + (파일 올리기)를 누르면 바로 첨부됩니다", Toast.LENGTH_LONG).show()
        } else if (!text.isNullOrBlank()) {
            getSystemService(ClipboardManager::class.java).setPrimaryClip(ClipData.newPlainText("공유", text))
            Toast.makeText(this, "복사됨 — 채널에서 붙여넣기 하세요", Toast.LENGTH_LONG).show()
        }
        setIntent(Intent(intent).setAction(Intent.ACTION_MAIN))
    }

    companion object {
        const val HOME = "https://discord.com/app"
        private const val DESKTOP_UA =
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/128.0.0.0 Safari/537.36"
        private val ALLOWED_ORIGINS = setOf(
            "https://discord.com", "https://ptb.discord.com", "https://canary.discord.com",
        )

        fun isDiscord(host: String?): Boolean {
            if (host == null) return false
            return host == "discord.com" || host.endsWith(".discord.com") || host == "discord.gg"
        }
    }
}
