import type { MetadataRoute } from 'next';

export default function manifest(): MetadataRoute.Manifest {
  return {
    name: 'Sori',
    short_name: 'Sori',
    description: '내 음악을 원본 그대로. 셀프호스팅 무손실 스트리밍.',
    start_url: '/',
    display: 'standalone',
    background_color: '#0e0e10',
    theme_color: '#0e0e10',
    icons: [{ src: '/icon.svg', sizes: 'any', type: 'image/svg+xml' }],
  };
}
