import type { ReactNode } from 'react';
import './globals.css';

export const metadata = {
  title: '동행',
  description: '교통약자 승하차 도움 쪽지를 팀이 함께 보는 앱.',
};

/**
 * 확대는 막지 않는다. 점수판과 달리 이 앱은 한 손으로 읽는 글이 대부분이고, 작은 글씨를
 * 키워 볼 수 있어야 하는 사람이 쓴다.
 */
export const viewport = {
  themeColor: '#1d3a6e',
  width: 'device-width',
  initialScale: 1,
  viewportFit: 'cover' as const,
};

export default function RootLayout({ children }: { children: ReactNode }) {
  return (
    <html lang="ko">
      <body>{children}</body>
    </html>
  );
}
