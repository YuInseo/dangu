import type { Metadata, Viewport } from 'next';
import type { ReactNode } from 'react';
import 'pretendard/dist/web/variable/pretendardvariable-dynamic-subset.css';
import './globals.css';

export const metadata: Metadata = {
  title: { default: 'Sori', template: '%s · Sori' },
  description: '내 음악을 원본 그대로. 셀프호스팅 무손실 스트리밍.',
  applicationName: 'Sori',
};

export const viewport: Viewport = {
  themeColor: '#0e0e10',
  colorScheme: 'dark',
  width: 'device-width',
  initialScale: 1,
  viewportFit: 'cover',
};

export default function RootLayout({ children }: { children: ReactNode }) {
  return (
    <html lang="ko">
      <body>{children}</body>
    </html>
  );
}
