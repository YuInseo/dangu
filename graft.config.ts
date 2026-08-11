import { defineConfig } from 'graft';

/**
 * 당구 점수판 — 설정 전부.
 *
 * 웹과 안드로이드 앱이 같은 소스에서 나온다. 여기 있는 건 웹으로 표현할 수 없는 것들뿐이고,
 * 이름·아이콘·테마는 `app/manifest.ts`에서 온다.
 */
export default defineConfig({
  /**
   * 앱 버전.
   *
   * 인앱 업데이트가 이 값과 GitHub 릴리스의 태그를 비교한다. 릴리스 워크플로가 태그에서
   * `GRAFT_PUBLIC_APP_VERSION`을 넣어 빌드하므로, CI에서 나온 APK는 항상 자기 태그를 안다.
   */
  version: '1.0.0',

  reactStrictMode: true,
  // 점수판은 전부 클라이언트 상태다. 서버가 캐시할 만한 게 없다.
  cacheComponents: false,

  /**
   * 클라이언트 번들에 그대로 박히는 값들.
   *
   * 여기 있는 것과 `.env.local`의 `GRAFT_PUBLIC_*`은 둘 다 공개된다 — Firebase 웹 설정은
   * 원래 공개해도 되는 값이고(보안은 Firestore 규칙이 담당한다), 저장소 이름도 마찬가지다.
   */
  env: {
    // 릴리스를 확인할 저장소. 아래 GITHUB_REPO를 본인 것으로 바꾸면 업데이트 버튼이 살아난다.
    GRAFT_PUBLIC_GITHUB_REPO: process.env.GRAFT_PUBLIC_GITHUB_REPO ?? '',
    // CI가 태그에서 채운다. 로컬에서는 개발 빌드임이 드러나는 값이 정직하다.
    GRAFT_PUBLIC_APP_VERSION: process.env.GRAFT_PUBLIC_APP_VERSION ?? '0.0.0-dev',

    /**
     * Firebase 웹 설정.
     *
     * 이 프로젝트의 값이 기본값으로 적혀 있다. 시크릿이나 `.env.local`이 있으면 그쪽이
     * 이기고, 없으면 여기 적힌 값으로 빌드된다 — 사이드로딩으로 배포하는 앱이라
     * "환경변수를 넣고 다시 빌드하세요"가 통하지 않고, 시크릿이 비면 로그인과 클라우드
     * 저장이 조용히 꺼진 APK가 나가기 때문이다.
     *
     * 저장소에 적혀 있어도 되는 값이다. Firebase 웹 설정은 "어느 프로젝트인지"를 말할
     * 뿐이고, 누가 무엇을 읽고 쓸 수 있는지는 `firestore.rules`가 정한다. 그래서
     * 이 값들이 공개된 상태를 전제로 규칙을 짠다 — 자기 문서만, 로그인한 사람만.
     *
     * `??`가 아니라 `||`인 것은 CI 때문이다. 워크플로가 시크릿을 job 환경으로 올리므로,
     * 시크릿이 없어도 변수는 *빈 문자열로 정의*된다 — `??`는 그걸 값으로 받아들여
     * 기본값에 닿지 못한다.
     */
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

  /**
   * §E4 — 안드로이드 패키징.
   *
   * `enabled`면 평범한 `graft build`가 네이티브 페이로드까지 만든다. 웹과 앱이 서로
   * 뒤처질 수 없다는 뜻이라, 점수판 로직이 두 벌이 되지 않는다.
   */
  mobile: {
    enabled: true,
    appId: 'com.dangu.score',
    appName: '당구 점수판',
    // 당구장은 지하가 많다. 네트워크 없이도 게임은 끝까지 돌아가야 한다.
    offline: true,

    /**
     * 진동만 선언한다. 점수를 누를 때 손끝 피드백이 있어야 화면을 보지 않고도 눌렀는지 안다.
     * 카메라·위치 같은 건 쓰지 않으므로 선언하지 않는다 — 선언하지 않은 권한은 매니페스트에
     * 들어가지 않고, 스토어 심사에서 설명할 것도 없다.
     */
    capabilities: ['vibration', 'network'],

    /**
     * 구글 로그인은 기기 권한이 아니라 서비스 SDK라서 capability로 매핑할 수 없다.
     * 이름을 직접 적는 자리가 여기다.
     *
     * 안드로이드 WebView 안에서는 구글 OAuth 팝업이 막혀 있다(`disallowed_useragent`).
     * 그래서 앱에서는 이 플러그인이 네이티브 로그인 창을 띄우고, 받은 idToken을 그대로
     * Firebase에 넘긴다. 웹에서는 팝업을 쓴다. 호출부는 `lib/auth.ts` 한 곳뿐이다.
     */
    plugins: [
      '@capacitor-firebase/authentication',
      /**
       * 상태 표시줄.
       *
       * targetSdk 35부터 안드로이드는 앱을 edge-to-edge로 그린다 — WebView가 상태
       * 표시줄 *아래*가 아니라 그 자리까지 채우고, 시계와 앱 제목이 겹친다. WebView는
       * 상태 표시줄 높이를 `env(safe-area-inset-top)`으로 알려 주지 않으므로 CSS만으로는
       * 해결되지 않는다. 이 플러그인이 겹침을 끄는 유일한 방법이다.
       */
      '@capacitor/status-bar',
      /**
       * 조용한 업데이트.
       *
       * 안드로이드는 사이드로딩한 앱이 APK를 소리 없이 설치하도록 두지 않는다. 그런데
       * 이 앱에서 실제로 자주 바뀌는 것은 APK가 아니라 그 안의 웹 자산이고, 그건 이
       * 플러그인이 갈아끼울 수 있다. 릴리스에 붙은 zip을 받아 다음 실행에 적용한다.
       *
       * 설정은 `mobile/overlay/capacitor.config.json`에 있다. 특히 `autoUpdate: false`가
       * 중요하다 — 기본값이면 플러그인이 자기 회사 서버에 기기 정보를 물으러 간다.
       * 점수판이 남의 서버에 말을 걸 이유는 없고, 업데이트는 우리 릴리스에서 온다.
       */
      '@capgo/capacitor-updater',
      /**
       * 소리 내어 읽기.
       *
       * 안드로이드 WebView에는 `speechSynthesis`가 없다 — 크롬에는 있어서 브라우저로
       * 열면 그대로 들리지만, 앱 안에서는 같은 코드가 조용히 아무 일도 하지 않는다.
       * 기기에 이미 깔려 있는 시스템 TTS 엔진에 닿는 길이 이 플러그인뿐이다.
       *
       * 부르는 곳은 `lib/speech.ts` 한 곳이고, 없으면 웹 경로로 떨어진다.
       */
      '@capacitor-community/text-to-speech',
    ],

    build: 'auto',
    android: {
      /**
       * 34에 머무는 이유는 하나다.
       *
       * 안드로이드 15는 targetSdk 35인 앱을 강제로 edge-to-edge로 그린다 — WebView가
       * 상태 표시줄 자리까지 차지해서 앱 제목이 시계 위에 겹치고, StatusBar 플러그인의
       * `setOverlaysWebView(false)`는 그 버전에서 무시된다. 제대로 대응하려면 시스템
       * 인셋을 WebView에 넘겨 주는 플러그인이 필요한데, 그건 Capacitor 7부터다.
       *
       * 이 앱은 GitHub 릴리스로 사이드로딩하므로 Play의 targetSdk 기한에 매이지 않는다.
       * Play에 올릴 때가 오면 Capacitor를 7로 올리고 35로 되돌리면 된다.
       */
      targetSdk: 34,
      // 사이드로딩으로 배포한다: GitHub 릴리스에 올린 APK를 앱이 직접 받는다.
      // Play 스토어에 올릴 때가 오면 'aab'로 바꾸면 된다.
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
