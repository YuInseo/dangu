/**
 * 신호 경로 — 원본 파일에서 스피커까지 소리가 어떤 손을 거치는지.
 *
 * 단계마다 품질을 매기고, 전체 품질은 가장 낮은 단계를 따른다. 등급은 룬의 관례를
 * 따랐다: 아무것도 바뀌지 않으면 무손실, 샘플이 바뀌면(리샘플링, DSD→PCM) 고품질,
 * 일부러 소리를 만지면(디지털 볼륨) DSP, 원본부터 손실 압축이면 손실.
 */
import type { AudioFormat } from '../library/types';
import { formatDetail, khz } from '../quality';

export type SignalQuality = 'lossless' | 'high' | 'enhanced' | 'low';

export const SIGNAL_LABEL: Record<SignalQuality, string> = {
  lossless: '무손실',
  high: '고품질',
  enhanced: 'DSP 적용',
  low: '손실 압축',
};

const RANK: Record<SignalQuality, number> = { lossless: 3, enhanced: 2, high: 1, low: 0 };

export interface SignalStep {
  title: string;
  detail: string;
  quality: SignalQuality;
}

export interface SignalInput {
  format: AudioFormat;
  source: 'library' | 'live';
  transcoded: boolean;
  volume: number;
  muted: boolean;
  outputRate: number | null;
}

function browserName(): string {
  if (typeof navigator === 'undefined') return '브라우저';
  const ua = navigator.userAgent;
  if (/Edg\//.test(ua)) return 'Edge';
  if (/Firefox\//.test(ua)) return 'Firefox';
  if (/Chrome\//.test(ua)) return 'Chrome';
  if (/Safari\//.test(ua)) return 'Safari';
  return '브라우저';
}

export function signalPath(i: SignalInput): { quality: SignalQuality; steps: SignalStep[] } {
  const f = i.format;
  const base: SignalQuality = f.lossless ? 'lossless' : 'low';
  const steps: SignalStep[] = [];

  steps.push({
    title: `원본 · ${f.codec}`,
    detail: formatDetail(f) || (f.lossless ? '무손실' : '손실 압축'),
    quality: base,
  });

  // 브라우저에 실제로 도착하는 샘플레이트
  let rate = f.sampleRate;
  if (i.transcoded) {
    const dsd = f.codec === 'DSD';
    const resampled = dsd || (f.sampleRate ?? 0) > 192000;
    if (resampled) rate = 176400;
    steps.push({
      title: 'Sori 서버 · ffmpeg',
      detail: dsd
        ? 'DSD → PCM 24비트 / 176.4kHz로 변환'
        : resampled
          ? `${f.codec} → FLAC, ${khz(f.sampleRate!)}kHz → 176.4kHz`
          : `${f.codec} → FLAC 변환 · 샘플은 그대로`,
      quality: resampled ? (f.lossless ? 'high' : 'low') : base,
    });
  } else {
    steps.push({
      title: i.source === 'live' ? 'archive.org' : 'Sori 서버',
      detail: '원본 파일을 바이트 그대로 전송',
      quality: base,
    });
  }

  steps.push({
    title: `${browserName()} 디코더`,
    detail: '32비트 부동소수점으로 풀기 · 손실 없음',
    quality: base,
  });

  if (i.muted || i.volume < 1) {
    steps.push({
      title: '디지털 볼륨',
      detail: i.muted ? '음소거' : `${Math.round(i.volume * 100)}% · 볼륨을 100%로 두면 이 단계가 빠집니다`,
      quality: base === 'low' ? 'low' : 'enhanced',
    });
  }

  if (i.outputRate) {
    const mismatch = rate && rate !== i.outputRate;
    steps.push({
      title: `출력 장치 · ${khz(i.outputRate)}kHz`,
      detail: mismatch
        ? `운영체제 설정이 ${khz(i.outputRate)}kHz라 ${khz(rate!)}kHz에서 리샘플링됩니다`
        : rate
          ? '샘플레이트 일치 · 그대로 출력'
          : '기본 출력 장치',
      quality: mismatch ? (base === 'low' ? 'low' : 'high') : base,
    });
  } else {
    steps.push({ title: '출력 장치', detail: '재생을 시작하면 출력 샘플레이트를 확인합니다', quality: base });
  }

  const quality = steps.reduce<SignalQuality>((q, s) => (RANK[s.quality] < RANK[q] ? s.quality : q), 'lossless');
  return { quality, steps };
}
