import type { ReactNode } from 'react';
import './globals.css';

export const metadata = {
  title: '당구 점수판',
  description: '4구 · 3구 · 포켓볼 점수판. 핸디, 타이머, 기록까지.',
};

/**
 * 뷰포트.
 *
 * `viewport-fit=cover`가 노치 아래까지 배경을 채우고, safe-area 값은 CSS가 읽는다.
 * `user-scalable=no`는 보통 접근성 때문에 피할 것이지만 점수판은 예외다 — 큐를 든 손이
 * 두 손가락으로 스치면 화면이 확대된 채 게임이 끝날 때까지 남는다.
 */
export const viewport = {
  themeColor: '#0f1115',
  width: 'device-width',
  initialScale: 1,
  maximumScale: 1,
  userScalable: false,
  viewportFit: 'cover' as const,
};

export default function RootLayout({ children }: { children: ReactNode }) {
  return (
    <html lang="ko">
      <body>{children}</body>
    </html>
  );
}
