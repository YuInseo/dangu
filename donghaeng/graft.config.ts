import { defineConfig } from 'graft';

/**
 * 동행 — 설정 전부.
 *
 * 당구 점수판과 같은 저장소에 살지만 서로 다른 앱이다. 패키지 이름(`appId`)이 다르므로
 * 한 폰에 둘 다 깔리고, 서로를 덮어쓰지 않는다.
 */
export default defineConfig({
  version: '0.1.0',

  reactStrictMode: true,
  // 쪽지는 전부 Firestore에서 실시간으로 온다. 서버가 캐시할 만한 게 없다.
  cacheComponents: false,

  /**
   * 클라이언트 번들에 그대로 박히는 값들.
   *
   * Firebase 프로젝트는 당구 점수판과 같은 것을 쓴다 — 컬렉션이 `teams/`로 갈라져 있고,
   * 규칙도 그 경로에만 따로 붙는다(`firestore.rules`). 다른 프로젝트로 옮기려면 시크릿이나
   * `.env.local`에 값을 넣으면 그쪽이 이긴다.
   *
   * `??`가 아니라 `||`인 이유는 CI다. 시크릿이 없어도 변수가 빈 문자열로 정의된다.
   */
  env: {
    GRAFT_PUBLIC_APP_VERSION: process.env.GRAFT_PUBLIC_APP_VERSION ?? '0.0.0-dev',
    GRAFT_PUBLIC_FIREBASE_API_KEY:
      process.env.GRAFT_PUBLIC_FIREBASE_API_KEY || 'AIzaSyCrGaNU7hKTHM2Cqc5LRNgeeTRBWVkbV8w',
    GRAFT_PUBLIC_FIREBASE_AUTH_DOMAIN:
      process.env.GRAFT_PUBLIC_FIREBASE_AUTH_DOMAIN || 'moneywalk-551ca.firebaseapp.com',
    GRAFT_PUBLIC_FIREBASE_PROJECT_ID:
      process.env.GRAFT_PUBLIC_FIREBASE_PROJECT_ID || 'moneywalk-551ca',
    GRAFT_PUBLIC_FIREBASE_APP_ID:
      process.env.GRAFT_PUBLIC_FIREBASE_APP_ID || '1:1073558096483:android:faf99adce285e6e2d08656',
    GRAFT_PUBLIC_FIREBASE_STORAGE_BUCKET:
      process.env.GRAFT_PUBLIC_FIREBASE_STORAGE_BUCKET || 'moneywalk-551ca.firebasestorage.app',
    GRAFT_PUBLIC_FIREBASE_MESSAGING_SENDER_ID:
      process.env.GRAFT_PUBLIC_FIREBASE_MESSAGING_SENDER_ID || '1073558096483',
  },

  mobile: {
    enabled: true,
    appId: 'com.donghaeng.app',
    appName: '동행',
    // 승강장 끝이나 지하 통로에서는 신호가 끊긴다. 쪽지는 기기에 먼저 쓰이고, 붙으면 올라간다.
    offline: true,

    // 새 쪽지가 오면 손에 알린다. 카메라·위치는 쓰지 않으므로 선언하지 않는다.
    capabilities: ['vibration', 'network'],

    /**
     * 로그인은 Firebase 익명 인증이라 전부 자바스크립트다 — 네이티브 로그인 플러그인이
     * 필요 없고, 그래서 `google-services.json`도 필요 없다.
     *
     * 플러그인은 상태 표시줄 색, 공유 시트, 뒤로 가기 버튼 셋뿐이다.
     */
    plugins: [
      '@capacitor/status-bar',
      /**
       * 공유 시트. 안드로이드 WebView에는 `navigator.share`가 없어서, 이게 없으면 공유
       * 버튼이 클립보드 복사로 떨어진다. 카톡으로 쪽지를 넘기는 게 이 버튼의 쓰임이라
       * 시트가 바로 떠야 한다.
       */
      '@capacitor/share',
      // 뒤로 가기 버튼. 쪽지를 적던 시트가 열려 있을 때 앱이 꺼지지 않고 시트만 닫히게.
      '@capacitor/app',
    ],

    build: 'auto',
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
