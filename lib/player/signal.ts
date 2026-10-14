/**
 * 신호 경로 — 원본 파일에서 스피커까지 소리가 어떤 손을 거치는지.
 *
 * 룬의 네 등급을 그대로 쓴다.
 *   무손실(보라 별)   아무것도 바뀌지 않는다
 *   향상(파랑 별)     사용자가 원해서 소리를 만진다 — 볼륨 레벨링, 헤드룸, EQ, 크로스피드 …
 *   고품질(초록 점)   비트퍼펙트를 보장할 수 없다 — 샘플레이트 변환, 비트 깊이 줄이기, OS 믹서
 *   저품질(노랑 점)   손실 압축 원본
 *
 * 단계마다 자기 품질을 갖고, 전체 등급은 가장 낮은 단계를 따른다. 브라우저는 출력 장치를
 * 독점으로 열 수 없어 늘 운영체제 믹서를 지나므로, 룬의 "System Output"처럼 마지막 단계는
 * 고품질이다.
 */
import type { AudioFormat } from '../library/types';
import { planDsp, fmtDb, type Caps, type Stage, type StageQuality } from '../dsp/plan';
import type { DspRequest, Loudness } from '../dsp/types';
import { formatDetail, khz } from '../quality';

export type SignalQuality = StageQuality;
export type SignalStep = Stage;

export const SIGNAL_LABEL: Record<SignalQuality, string> = {
  lossless: '무손실',
  enhanced: '향상됨',
  high: '고품질',
  low: '저품질',
};

const RANK: Record<SignalQuality, number> = { lossless: 3, enhanced: 2, high: 1, low: 0 };

export interface SignalInput {
  format: AudioFormat;
  source: 'library' | 'live';
  /** 브라우저가 못 여는 형식이라 서버가 FLAC으로 바꿔 보내는 중 */
  transcoded: boolean;
  /** DSP 요청. 있으면 서버가 64비트 체인을 거쳐 보낸다 */
  dsp: DspRequest | null;
  loudness?: Loudness;
  caps?: Caps;
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

export function worst(steps: Stage[]): SignalQuality {
  return steps.reduce<SignalQuality>((q, s) => (RANK[s.quality] < RANK[q] ? s.quality : q), 'lossless');
}

export function signalPath(i: SignalInput): { quality: SignalQuality; steps: SignalStep[] } {
  const f = i.format;
  const steps: Stage[] = [];

  steps.push({
    title: `원본 · ${f.codec}`,
    detail: formatDetail(f) || (f.lossless ? '무손실' : '손실 압축'),
    quality: f.lossless ? 'lossless' : 'low',
  });

  // 브라우저에 도착하는 샘플레이트
  let rate = f.sampleRate;
  if (i.dsp) {
    const plan = planDsp(i.dsp, f, i.loudness, { caps: i.caps });
    steps.push(...plan.stages);
    rate = plan.outRate ?? rate;
    steps.push({
      title: 'Sori 서버',
      detail: `FLAC ${plan.outBits}비트${rate ? ` / ${khz(rate)}kHz` : ''} 무손실 전송`,
      quality: 'lossless',
    });
  } else if (i.transcoded) {
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
      quality: resampled ? 'high' : 'lossless',
    });
  } else {
    steps.push({
      title: i.source === 'live' ? 'archive.org' : 'Sori 서버',
      detail: '원본 파일을 바이트 그대로 전송',
      quality: 'lossless',
    });
  }

  steps.push({ title: `${browserName()} 디코더`, detail: '32비트 부동소수점으로 풀기 · 손실 없음', quality: 'lossless' });

  if (i.muted || i.volume < 1) {
    steps.push({
      title: '볼륨',
      detail: i.muted ? '음소거' : `${fmtDb(20 * Math.log10(Math.max(i.volume, 1e-4)))} · 브라우저 디지털 볼륨`,
      quality: 'enhanced',
    });
  }

  if (i.outputRate && rate && rate !== i.outputRate) {
    steps.push({
      title: '샘플레이트 변환 · 브라우저',
      detail: `${khz(rate)}kHz → ${khz(i.outputRate)}kHz · DSP에서 "출력 장치에 맞춤"을 켜면 서버가 대신 정밀하게 바꿉니다`,
      quality: 'high',
    });
  }

  steps.push({
    title: '시스템 출력',
    detail: `OS 믹서${i.outputRate ? ` · ${khz(i.outputRate)}kHz` : ''} · 공유 모드라 비트퍼펙트를 보장할 수 없습니다`,
    quality: 'high',
  });

  return { quality: worst(steps), steps };
}
