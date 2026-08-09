/**
 * 앱의 이름·아이콘·테마는 여기 한 곳에서만 정한다.
 *
 * 웹 매니페스트이자 안드로이드 빌드의 입력이다 — 런처 이름, 스플래시 색, PWA 설치
 * 아이콘이 전부 이 파일에서 나오므로 세 군데가 서로 다를 수가 없다.
 */
export default function manifest() {
  return {
    name: '당구 점수판',
    short_name: '점수판',
    description: '4구 · 3구 · 포켓볼 점수판. 핸디, 타이머, 기록.',
    start_url: '/',
    scope: '/',
    display: 'standalone',
    // 점수판은 세로로 든 폰에 맞춰 설계했다. 가로도 되지만 기본은 세로다.
    orientation: 'portrait',
    background_color: '#0f1115',
    theme_color: '#0f1115',
    icons: [
      { src: '/icon-192.png', sizes: '192x192', type: 'image/png', purpose: 'any' },
      { src: '/icon-512.png', sizes: '512x512', type: 'image/png', purpose: 'maskable' },
    ],
  };
}
