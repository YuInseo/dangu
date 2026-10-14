import type { NextConfig } from 'next';

const config: NextConfig = {
  // 태그 파서와 이미지 처리는 네이티브 모듈과 파일 시스템을 쓴다. 번들에 넣지 않고 그대로 부른다.
  serverExternalPackages: ['music-metadata', 'sharp'],
  // 도커 이미지는 standalone 출력으로 작게 만든다. 평소 `next start`에는 필요 없다.
  output: process.env.SORI_STANDALONE ? 'standalone' : undefined,
  // 음악과 캐시는 실행할 때 읽는 데이터다. 빌드 결과물에 딸려 들어가면 안 된다.
  outputFileTracingExcludes: { '*': ['music/**', '.sori/**'] },
  poweredByHeader: false,
  reactStrictMode: true,
};

export default config;
