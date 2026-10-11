/** 앱의 이름·아이콘·테마. 웹 매니페스트이자 안드로이드 빌드의 입력이다. */
export default function manifest() {
  return {
    name: '동행',
    short_name: '동행',
    description: '교통약자 승하차 도움 쪽지를 팀이 함께 보는 앱.',
    start_url: '/',
    scope: '/',
    display: 'standalone',
    orientation: 'portrait',
    background_color: '#f3f5f9',
    theme_color: '#1d3a6e',
    icons: [
      { src: '/icon-192.png', sizes: '192x192', type: 'image/png', purpose: 'any' },
      { src: '/icon-512.png', sizes: '512x512', type: 'image/png', purpose: 'maskable' },
    ],
  };
}
