import { defineConfig } from 'graft';

/**
 * 동행 — 설정 전부.
 *
 * 당구 점수판과 같은 저장소에 살지만 서로 다른 앱이다.
 */
export default defineConfig({
  /**
   * 버전. CI가 빌드 번호로 채운다(`0.1.<run_number>`) — 업데이트가 이 값으로 새것을 알아보고,
   * 안드로이드의 versionCode도 여기서 나와서 새 APK가 옛것 위에 깔린다.
   */
  version: process.env.GRAFT_PUBLIC_APP_VERSION || '0.1.0',

  reactStrictMode: true,
  // 쪽지는 전부 Firestore에서 실시간으로 온다. 서버가 캐시할 만한 게 없다.
  cacheComponents: false,

  /**
   * 클라이언트 번들에 그대로 박히는 값들.
   *
   * Firebase 프로젝트는 동행 전용인 motion-8b939("With")다. 당구 점수판(moneywalk-551ca)과
   * 데이터·로그인이 섞이지 않는다. 다른 프로젝트로 옮기려면 `.env.local`에 값을 넣으면 그쪽이
   * 이긴다. (CI는 이 값을 덮어쓰지 않는다 — 저장소의 FIREBASE_* 시크릿은 점수판의 것이다.)
   *
   * `??`가 아니라 `||`인 이유는 CI다. 시크릿이 없어도 변수가 빈 문자열로 정의된다.
   */
  env: {
    GRAFT_PUBLIC_APP_VERSION: process.env.GRAFT_PUBLIC_APP_VERSION || '0.0.0-dev',
    // 업데이트를 확인할 저장소와, 이 빌드의 네이티브 껍데기 지문(lib/update.ts).
    GRAFT_PUBLIC_GITHUB_REPO: process.env.GRAFT_PUBLIC_GITHUB_REPO || 'YuInseo/dangu',
    GRAFT_PUBLIC_NATIVE_KEY: process.env.GRAFT_PUBLIC_NATIVE_KEY || '',
    GRAFT_PUBLIC_FIREBASE_API_KEY:
      process.env.GRAFT_PUBLIC_FIREBASE_API_KEY || 'AIzaSyARET0dMJUBuveOwmxMPa9rVOOJHRCQGBc',
    GRAFT_PUBLIC_FIREBASE_AUTH_DOMAIN:
      process.env.GRAFT_PUBLIC_FIREBASE_AUTH_DOMAIN || 'motion-8b939.firebaseapp.com',
    GRAFT_PUBLIC_FIREBASE_PROJECT_ID:
      process.env.GRAFT_PUBLIC_FIREBASE_PROJECT_ID || 'motion-8b939',
    GRAFT_PUBLIC_FIREBASE_APP_ID:
      process.env.GRAFT_PUBLIC_FIREBASE_APP_ID || '1:309766835892:android:be5960ccc0bf9ba54e91e1',
    GRAFT_PUBLIC_FIREBASE_STORAGE_BUCKET:
      process.env.GRAFT_PUBLIC_FIREBASE_STORAGE_BUCKET || 'motion-8b939.firebasestorage.app',
    GRAFT_PUBLIC_FIREBASE_MESSAGING_SENDER_ID:
      process.env.GRAFT_PUBLIC_FIREBASE_MESSAGING_SENDER_ID || '309766835892',
  },

  mobile: {
    enabled: true,
    /**
     * 동행만의 패키지 이름. 당구 점수판(com.dangu.score)과 한 폰에 같이 깔린다.
     *
     * 앱 안의 구글 로그인 창은 Firebase 콘솔에 "패키지 이름 + 서명 키 지문"이 등록된 앱에서만
     * 열린다. 그래서 콘솔에 이 이름으로 안드로이드 앱을 등록하고 서명 키의 SHA-1을 넣어야 한다
     * (README). 등록하면 나오는 앱 ID는 `mobile/overlay/.../firebase.xml`의 `google_app_id`로.
     */
    appId: 'com.donghaeng.app',
    appName: '동행',
    // 승강장 끝이나 지하 통로에서는 신호가 끊긴다. 쪽지는 기기에 먼저 쓰이고, 붙으면 올라간다.
    offline: true,

    // 새 쪽지가 오면 손에 알린다. 위치는 쓰지 않으므로 선언하지 않는다.
    // `notifications`는 열차 시각 전에 울리는 쪽지 알림(lib/alarm.ts) — 앱이 꺼져 있어도 울린다.
    // `camera`는 승차권을 찍어 쪽지를 채우는 데(lib/ticket.ts) — @capacitor/camera가 따라온다.
    // 글자 인식은 mobile/native/android/TicketOcrPlugin.kt가 기기 안에서 한다.
    capabilities: ['vibration', 'network', 'notifications', 'camera'],

    plugins: [
      /**
       * 구글 로그인.
       *
       * 안드로이드 WebView 안에서는 구글 OAuth 팝업이 막혀 있다(`disallowed_useragent`).
       * 그래서 앱에서는 이 플러그인이 네이티브 로그인 창을 띄우고, 받은 idToken을 자바스크립트
       * SDK에 넘긴다(`skipNativeAuth`, `mobile/overlay/capacitor.config.json`).
       *
       * 이 플러그인은 구글 쪽 라이브러리를 기본으로 빼고 짓는다. 넣으라는 Gradle 변수
       * `rgcfaIncludeGoogle`은 CI가 생성된 프로젝트에 적는다(donghaeng.yml).
       */
      '@capacitor-firebase/authentication',
      '@capacitor/status-bar',
      /**
       * 공유 시트. 안드로이드 WebView에는 `navigator.share`가 없어서, 이게 없으면 공유
       * 버튼이 클립보드 복사로 떨어진다. 카톡으로 쪽지를 넘기는 게 이 버튼의 쓰임이라
       * 시트가 바로 떠야 한다.
       */
      '@capacitor/share',
      /**
       * 조용한 업데이트. 릴리스의 웹 번들을 받아 다음 실행에 적용한다(lib/update.ts).
       * 설정은 `mobile/overlay/capacitor.config.json` — `autoUpdate: false`가 중요하다.
       * 기본값이면 플러그인이 자기 회사 서버에 기기 정보를 물으러 간다.
       * 버전은 CI가 6.13.2로 못박는다(당구 점수판과 같은 이유: compileSdk 34).
       */
      '@capgo/capacitor-updater',
      // 뒤로 가기 버튼. 쪽지를 적던 시트가 열려 있을 때 앱이 꺼지지 않고 시트만 닫히게.
      '@capacitor/app',
    ],

    android: {
      // 당구 점수판과 같은 이유로 34에 머문다(그쪽 graft.config.ts의 주석).
      targetSdk: 34,
      format: 'apk',
      keystore: {
        fileEnv: 'GRAFT_ANDROID_KEYSTORE',
        aliasEnv: 'GRAFT_ANDROID_KEY_ALIAS',
        storePasswordEnv: 'GRAFT_ANDROID_KEYSTORE_PASSWORD',
        keyPasswordEnv: 'GRAFT_ANDROID_KEY_PASSWORD',
      },
    },
  },
});
