/**
 * 서버 설정. 전부 환경변수에서 온다 — `.env.local`이나 도커 `environment:`.
 */
import path from 'node:path';
import { spawn } from 'node:child_process';
import pkg from '../package.json';

/**
 * 실행할 때 정해지는 폴더. 빌드 도구가 기본값('./music')을 보고 그 안의 파일을 전부
 * 번들 추적에 넣으려 들지 않도록, 경로를 정적으로 읽을 수 없게 만든다.
 */
function runtimeDir(value: string | undefined, fallback: string): string {
  const cwd = /* turbopackIgnore: true */ process.cwd();
  return path.resolve(/* turbopackIgnore: true */ cwd, value || fallback);
}

export const config = {
  /** 음악이 있는 폴더. 읽기만 한다. */
  musicDir: runtimeDir(process.env.MUSIC_DIR, 'music'),
  /** 색인과 표지 캐시를 두는 곳. */
  dataDir: runtimeDir(process.env.DATA_DIR, '.sori'),
  /** 비어 있으면 로그인 없이 열린다. 집 밖으로 내놓을 거면 꼭 넣는다. */
  password: process.env.SORI_PASSWORD || '',
  ffmpeg: process.env.FFMPEG_PATH || 'ffmpeg',
  version: pkg.version,
};

let ffmpegCheck: Promise<boolean> | null = null;

/**
 * ffmpeg가 있는지. 브라우저가 못 여는 포맷(ALAC, DSD, AIFF 일부, WavPack, APE)을
 * FLAC으로 바꿔 보낼 때만 쓴다. 없어도 FLAC·WAV·MP3·AAC는 그대로 재생된다.
 */
export function hasFfmpeg(): Promise<boolean> {
  ffmpegCheck ??= new Promise((resolve) => {
    try {
      const p = spawn(/* turbopackIgnore: true */ config.ffmpeg, ['-version'], { stdio: 'ignore' });
      p.on('error', () => resolve(false));
      p.on('exit', (code) => resolve(code === 0));
    } catch {
      resolve(false);
    }
  });
  return ffmpegCheck;
}
