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

    /** 페이지가 어떤 상태인지 — 안 뜰 때 이유를 화면에 보여 주려고 */
    data class PageState(
        val progress: Int = 0,
        val url: String = "",
        val error: String? = null,
        val blank: String? = null,
        val console: List<String> = emptyList(),
    )
    val page = androidx.compose.runtime.mutableStateOf(PageState())
    private var blankCheck: Runnable? = null
    private var appliedUa = -1

    /** 영상·방송을 전체 화면으로 볼 때 WebView가 넘겨주는 뷰 */
    val fullscreen = androidx.compose.runtime.mutableStateOf<android.view.View?>(null)
    private lateinit var root: android.widget.FrameLayout
    private var fullscreenCallback: WebChromeClient.CustomViewCallback? = null

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
        scheduleBlankCheck(20_000)

        onBackPressedDispatcher.addCallback(this, object : OnBackPressedCallback(true) {
            override fun handleOnBackPressed() {
                // 디스코드 안에서 뒤로 갈 곳이 있으면 거기로, 없으면 앱을 끄지 않고 뒤로 보낸다
                // (연결이 살아 있어야 알림이 온다).
                when {
                    fullscreen.value != null -> exitFullscreen()
                    webView.canGoBack() -> webView.goBack()
                    else -> moveTaskToBack(true)
                }
            }
        })

        lifecycleScope.launch {
            prefs.state.collectLatest { s ->
                webView.settings.textZoom = s.textZoom
                installStartScript()
                webView.evaluateJavascript(Injector.applyCall(if (s.safeMode || sessionSafe) "" else currentCss(), s.wideLayout), null)
                webView.setBackgroundColor(pageBackground())
                if (::root.isInitialized) root.setBackgroundColor(pageBackground())
                applySystemBars()
                if (applyUserAgent()) webView.reload()
            }
        }

        if (prefs.value.notifications && Build.VERSION.SDK_INT >= 33 &&
            ContextCompat.checkSelfPermission(this, Manifest.permission.POST_NOTIFICATIONS) != PackageManager.PERMISSION_GRANTED
        ) {
            notificationPermission.launch(Manifest.permission.POST_NOTIFICATIONS)
        }

        // WebView를 Compose 안(AndroidView)에 넣으면 이 WebView는 화면에 한 픽셀도 그려지지 않았다
        // (에뮬레이터에서 확인: 페이지는 돌고 있는데 배경색조차 안 나옴). 그래서 WebView는 평범한
        // 뷰 계층에 바로 두고, Compose(떠 있는 단추·설정·안내 카드)는 그 위에 투명한 층으로 올린다.
        root = android.widget.FrameLayout(this).apply {
            setBackgroundColor(pageBackground())
            addView(webView, android.widget.FrameLayout.LayoutParams(-1, -1))
            addView(
                androidx.compose.ui.platform.ComposeView(this@MainActivity).apply {
                    setContent { LumenScreen(this@MainActivity) }
                },
                android.widget.FrameLayout.LayoutParams(-1, -1),
            )
        }
        // 상태 표시줄·내비게이션 막대·키보드만큼 WebView를 안쪽으로.
        androidx.core.view.ViewCompat.setOnApplyWindowInsetsListener(root) { _, insets ->
            val types = androidx.core.view.WindowInsetsCompat.Type.systemBars() or
                androidx.core.view.WindowInsetsCompat.Type.ime()
            val i = if (fullscreen.value != null) androidx.core.graphics.Insets.NONE else insets.getInsets(types)
            (webView.layoutParams as android.widget.FrameLayout.LayoutParams).apply {
                setMargins(i.left, i.top, i.right, i.bottom)
            }
            webView.requestLayout()
            insets
        }
        setContentView(root)
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

    fun exitFullscreen() {
        fullscreenCallback?.onCustomViewHidden()
        fullscreenCallback = null
        fullscreen.value?.let { root.removeView(it) }
        fullscreen.value = null
        showSystemBars(true)
    }

    private fun showSystemBars(show: Boolean) {
        val c = androidx.core.view.WindowCompat.getInsetsController(window, window.decorView)
        val bars = androidx.core.view.WindowInsetsCompat.Type.systemBars()
        if (show) c.show(bars) else {
            c.systemBarsBehavior = androidx.core.view.WindowInsetsControllerCompat.BEHAVIOR_SHOW_TRANSIENT_BARS_BY_SWIPE
            c.hide(bars)
        }
    }

    override fun onDestroy() {
        CallService.set(this, false)
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
    private fun pageBackground(): Int {
        val t = Themes.byId(prefs.value.themeId)
        return if (t.id == "original") 0xFF313338.toInt() else t.chat.toInt()
    }

    private fun currentCss(): String {
        val s = prefs.value
        return Themes.css(Themes.byId(s.themeId), s)
    }

    private fun installStartScript() {
        val s = prefs.value
        startScript?.remove()
        startScript = null
        if (s.safeMode || sessionSafe) {
            lastScript = ""
            return
        }
        val script = Injector.documentStart(currentCss(), s.wideLayout, s.notifications)
        if (WebViewFeature.isFeatureSupported(WebViewFeature.DOCUMENT_START_SCRIPT)) {
            startScript = runCatching { WebViewCompat.addDocumentStartJavaScript(webView, script, ALLOWED_ORIGINS) }
                .onFailure { addConsole("스크립트 등록 실패: ${it.message}") }
                .getOrNull()
        }
        lastScript = script
    }

    /** 브라우저 종류를 설정에 맞춘다. 바뀌었으면 true. */
    private fun applyUserAgent(): Boolean {
        val mode = prefs.value.uaMode
        if (mode == appliedUa) return false
        val first = appliedUa == -1
        appliedUa = mode
        webView.settings.userAgentString = when (mode) {
            1 -> null // WebView 기본
            2 -> MOBILE_UA
            else -> DESKTOP_UA
        }
        return !first
    }

    fun retry(safe: Boolean? = null, nextUa: Boolean = false) {
        prefs.update {
            copy(
                safeMode = safe ?: safeMode,
                uaMode = if (nextUa) (uaMode + 1) % 3 else uaMode,
            )
        }
        page.value = PageState()
        sessionSafe = false
        installStartScript()
        scheduleBlankCheck(15_000)
        if (nextUa) return // 브라우저 종류가 바뀌면 설정을 보는 쪽이 새로고침한다
        if (webView.url.isNullOrEmpty() || !isDiscord(Uri.parse(webView.url).host)) webView.loadUrl(HOME) else webView.reload()
    }

    fun openInBrowser() = openExternal(Uri.parse(webView.url?.takeIf { it.startsWith("https://") } ?: HOME))

    private fun addConsole(line: String) {
        page.value = page.value.copy(console = (page.value.console + line).takeLast(8))
    }

    /** 이번 실행에서만 주입을 끈다 — 빈 화면이면 한 번 자동으로 켠다. */
    private var sessionSafe = false
    private var autoSafeTried = false

    private fun diag(): String {
        val pkg = WebViewCompat.getCurrentWebViewPackage(this)
        return "WebView ${pkg?.packageName?.substringAfterLast('.') ?: "?"} ${pkg?.versionName ?: "?"} · " +
            "크기 ${webView.width}×${webView.height} · 붙음 ${webView.isAttachedToWindow} · 진행 ${page.value.progress}%"
    }

    /**
     * 화면이 제대로 떴는지 본다. 페이지가 끝났다는 신호가 안 와도 켠 뒤 한 번은 반드시 본다 —
     * 아무 안내 없이 검은 화면만 남지 않도록.
     */
    private fun scheduleBlankCheck(delayMs: Long = 10_000) {
        blankCheck?.let { webView.removeCallbacks(it) }
        val check = Runnable {
            if (page.value.error != null) return@Runnable
            webView.evaluateJavascript(
                "(function(){var m=document.getElementById('app-mount');" +
                    "var t=document.body?document.body.innerText.trim().length:0;" +
                    "return JSON.stringify({title:document.title,mount:m?m.childElementCount:-1,text:t,href:location.href});})()"
            ) { raw ->
                val json = runCatching { org.json.JSONObject(org.json.JSONTokener(raw).nextValue() as String) }.getOrNull()
                val text = json?.optInt("text") ?: -1
                if (text >= 3) {
                    if (page.value.blank != null) page.value = page.value.copy(blank = null)
                    if (sessionSafe && !prefs.value.safeMode) {
                        Toast.makeText(this, "테마를 끄니 화면이 떴어요 — 이번 실행은 테마 없이 보여요", Toast.LENGTH_LONG).show()
                    }
                    return@evaluateJavascript
                }
                val what = if (json == null) "페이지 스크립트가 응답하지 않음" else
                    "제목 '${json.optString("title")}' · app-mount 자식 ${json.optInt("mount")} · 글자 $text"
                // 한 번은 테마·스크립트를 끄고 스스로 다시 해 본다.
                if (!prefs.value.safeMode && !autoSafeTried) {
                    autoSafeTried = true
                    sessionSafe = true
                    addConsole("빈 화면($what) → 테마 끄고 자동으로 다시 시도")
                    installStartScript()
                    webView.reload()
                    scheduleBlankCheck(15_000)
                    return@evaluateJavascript
                }
                page.value = page.value.copy(blank = "$what\n${diag()}")
            }
        }
        blankCheck = check
        webView.postDelayed(check, delayMs)
    }

    private var lastScript: String = ""

    @SuppressLint("SetJavaScriptEnabled")
    private fun createWebView(): WebView {
        val wv = WebView(this)
        // 투명 배경 WebView는 일부 기기(삼성 등)에서 내용을 아예 그리지 않는다. 불투명하게.
        wv.setBackgroundColor(pageBackground())
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
            // 브라우저 종류는 applyUserAgent()가 설정대로 정한다.
            textZoom = prefs.value.textZoom
        }
        CookieManager.getInstance().apply {
            setAcceptCookie(true)
            setAcceptThirdPartyCookies(wv, true) // 로그인 캡차(hCaptcha)
        }
        webView = wv
        applyUserAgent()
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
                page.value = PageState(progress = 5, url = url.orEmpty(), console = page.value.console)
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
                page.value = page.value.copy(url = url.orEmpty())
                scheduleBlankCheck()
            }

            override fun onReceivedError(view: WebView, request: WebResourceRequest, error: android.webkit.WebResourceError) {
                if (request.isForMainFrame) {
                    page.value = page.value.copy(error = "${error.description} (${error.errorCode})")
                }
            }

            override fun onReceivedHttpError(view: WebView, request: WebResourceRequest, response: android.webkit.WebResourceResponse) {
                if (request.isForMainFrame && response.statusCode >= 400) {
                    page.value = page.value.copy(error = "HTTP ${response.statusCode} ${response.reasonPhrase.orEmpty()}")
                }
            }

            override fun doUpdateVisitedHistory(view: WebView, url: String?, isReload: Boolean) {
                currentHost = url?.let { Uri.parse(it).host }
            }
        }

        wv.webChromeClient = object : WebChromeClient() {
            override fun onProgressChanged(view: WebView, newProgress: Int) {
                page.value = page.value.copy(progress = newProgress)
            }

            override fun onConsoleMessage(message: android.webkit.ConsoleMessage): Boolean {
                if (message.messageLevel() == android.webkit.ConsoleMessage.MessageLevel.ERROR) {
                    addConsole(message.message().take(200))
                }
                return false
            }

            override fun onShowCustomView(view: android.view.View, callback: CustomViewCallback) {
                fullscreenCallback?.onCustomViewHidden()
                fullscreenCallback = callback
                fullscreen.value = view
                root.addView(view, android.widget.FrameLayout.LayoutParams(-1, -1))
                showSystemBars(false)
            }

            override fun onHideCustomView() {
                fullscreenCallback = null
                fullscreen.value?.let { root.removeView(it) }
                fullscreen.value = null
                showSystemBars(true)
            }

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
        private const val MOBILE_UA =
            "Mozilla/5.0 (Linux; Android 14; SM-S928N) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/128.0.0.0 Mobile Safari/537.36"
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
