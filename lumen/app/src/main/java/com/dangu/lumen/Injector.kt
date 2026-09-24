package com.dangu.lumen

import org.json.JSONObject

/**
 * 페이지에 넣는 스크립트.
 *
 * 문서가 시작될 때(디스코드 코드보다 먼저) 들어가서 테마 스타일을 붙이고, 알림 API를 앱의
 * 알림으로 이어 준다. 테마를 바꾸면 같은 함수를 다시 불러 스타일만 갈아 끼운다 — 새로고침 없이.
 */
object Injector {
    fun applyCall(css: String, wide: Boolean): String =
        "window.__lumen && window.__lumen.apply(${JSONObject.quote(css)}, $wide);"

    fun documentStart(css: String, wide: Boolean, notifications: Boolean): String = """
        (function () {
          if (window.__lumen) { window.__lumen.apply(${JSONObject.quote(css)}, $wide); return; }
          var L = window.__lumen = { css: ${JSONObject.quote(css)}, wide: $wide };

          function style() {
            var p = document.head || document.documentElement;
            if (!p) return;
            var s = document.getElementById('lumen-theme');
            if (!s) { s = document.createElement('style'); s.id = 'lumen-theme'; }
            if (s.textContent !== L.css) s.textContent = L.css;
            if (s.parentNode !== p) p.appendChild(s);
          }
          function viewport() {
            if (!document.head) return;
            var m = document.querySelector('meta[name="viewport"]');
            if (!m) { m = document.createElement('meta'); m.name = 'viewport'; document.head.appendChild(m); }
            var c = L.wide ? 'width=900' : 'width=device-width, initial-scale=1, maximum-scale=1, user-scalable=no';
            if (m.content !== c) m.content = c;
          }
          L.apply = function (css, wide) { L.css = css; L.wide = wide; style(); viewport(); };

          style();
          var headWatch = null;
          function watchHead() {
            if (headWatch || !document.head) return;
            headWatch = new MutationObserver(function () { style(); viewport(); });
            headWatch.observe(document.head, { childList: true });
            style(); viewport();
          }
          new MutationObserver(watchHead).observe(document.documentElement, { childList: true });
          document.addEventListener('DOMContentLoaded', function () { watchHead(); style(); viewport(); });

          ${if (notifications) NOTIFICATIONS else ""}
        })();
    """.trimIndent()

    // 안드로이드 WebView에는 Notification API가 없다. 디스코드가 부르는 모양 그대로 흉내 내서
    // 앱 알림으로 넘긴다. 디스코드는 창이 안 보일 때만 알림을 만든다.
    private const val NOTIFICATIONS = """
          if (window.LumenBridge) {
            var N = function (title, opts) {
              opts = opts || {};
              this.title = title; this.body = opts.body || ''; this.tag = opts.tag || '';
              this.onclick = null; this.onclose = null; this.onshow = null; this.onerror = null;
              try { LumenBridge.notify(String(title || ''), String(this.body), String(this.tag)); } catch (e) {}
            };
            N.permission = 'granted';
            N.maxActions = 0;
            N.requestPermission = function (cb) { if (cb) cb('granted'); return Promise.resolve('granted'); };
            N.prototype.close = function () {};
            N.prototype.addEventListener = function () {};
            N.prototype.removeEventListener = function () {};
            try { Object.defineProperty(window, 'Notification', { value: N, configurable: true, writable: true }); }
            catch (e) { window.Notification = N; }
          }
    """
}
