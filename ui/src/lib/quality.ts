/**
 * 음질을 말로 옮기는 곳. 서버와 브라우저가 같이 쓴다.
 */
import type { AudioFormat, QualityTier } from './library/types';

export function tierOf(f: AudioFormat): QualityTier {
  if (!f.lossless) return 'lossy';
  if ((f.bitDepth ?? 16) > 16 || (f.sampleRate ?? 44100) > 48000) return 'hires';
  return 'lossless';
}

export const TIER_LABEL: Record<QualityTier, string> = {
  hires: 'Hi-Res',
  lossless: '무손실',
  lossy: '손실 압축',
};

/** 44100 → "44.1", 96000 → "96". */
export function khz(sampleRate: number): string {
  const k = sampleRate / 1000;
  return Number.isInteger(k) ? String(k) : k.toFixed(1).replace(/\.0$/, '');
}

/** DSD는 배율로 부른다. 2.8224MHz가 DSD64. */
function dsdLabel(sampleRate?: number): string {
  if (!sampleRate) return 'DSD';
  return `DSD${Math.round(sampleRate / 44100)}`;
}

/** "FLAC 24/96", "MP3 320k", "DSD64" 처럼 짧게. */
export function formatLabel(f: AudioFormat): string {
  if (f.codec === 'DSD') return dsdLabel(f.sampleRate);
  if (f.lossless) {
    if (f.bitDepth && f.sampleRate) return `${f.codec} ${f.bitDepth}/${khz(f.sampleRate)}`;
    if (f.sampleRate) return `${f.codec} ${khz(f.sampleRate)}kHz`;
    if (f.bitDepth) return `${f.codec} ${f.bitDepth}-bit`;
    return f.codec;
  }
  if (f.bitrate) return `${f.codec} ${Math.round(f.bitrate / 1000)}k`;
  return f.codec;
}

/** 신호 경로에 쓰는 긴 설명. "24비트 · 96kHz · 스테레오". */
export function formatDetail(f: AudioFormat): string {
  const parts: string[] = [];
  if (f.codec === 'DSD') parts.push(f.sampleRate ? `${(f.sampleRate / 1e6).toFixed(4)}MHz` : 'DSD');
  else {
    if (f.bitDepth) parts.push(`${f.bitDepth}비트`);
    if (f.sampleRate) parts.push(`${khz(f.sampleRate)}kHz`);
    if (!f.lossless && f.bitrate) parts.push(`${Math.round(f.bitrate / 1000)}kbps`);
  }
  if (f.channels) parts.push(f.channels === 1 ? '모노' : f.channels === 2 ? '스테레오' : `${f.channels}채널`);
  return parts.join(' · ');
}

/** 두 포맷 중 더 좋은 쪽. 앨범 배지에 쓴다. */
export function betterFormat(a: AudioFormat, b: AudioFormat): AudioFormat {
  const rank = (f: AudioFormat) =>
    (f.lossless ? 1e12 : 0) + (f.bitDepth ?? 0) * 1e7 + (f.sampleRate ?? 0) + (f.lossless ? 0 : (f.bitrate ?? 0) / 1000);
  return rank(b) > rank(a) ? b : a;
}

/** `audio/mp4; codecs="alac"` → `audio/mp4`. 응답 헤더에는 파라미터를 붙이지 않는다. */
export function baseMime(mime: string): string {
  return mime.split(';')[0]!.trim();
}
