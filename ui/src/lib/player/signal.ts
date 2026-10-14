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
import type { OutputInfo, SourceInfo } from './native';

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
  /** 이 기기의 장치로 직접 틀고 있으면 소리 서버가 알려 준 것 */
  native?: { output: OutputInfo | null; source: SourceInfo | null; resampling: [number, number] | null } | null;
}

const FORMAT_LABEL: Record<OutputInfo['format'], string> = {
  f64: '64비트 부동소수점',
  f32: '32비트 부동소수점',
  i32: '32비트 정수',
  i24: '24비트 정수',
  i16: '16비트 정수',
};

/** 소리 엔진(러스트)이 이 기기의 장치로 직접 틀 때. */
function nativePath(i: SignalInput, n: NonNullable<SignalInput['native']>): Stage[] {
  const f = i.format;
  const steps: Stage[] = [];
  steps.push({
    title: `원본 · ${f.codec}`,
    detail: formatDetail(f) || (f.lossless ? '무손실' : '손실 압축'),
    quality: f.lossless ? 'lossless' : 'low',
  });
  const src = n.source;
  if (src?.dsdRate) {
    steps.push({
      title: 'DSD → PCM · 소리 엔진',
      detail: `DSD${Math.round(src.dsdRate / 44100)} → ${khz(src.rate)}kHz PCM · 120dB 데시메이션 필터`,
      quality: 'high',
    });
  }
  steps.push({ title: '소리 엔진 · 64비트', detail: '64비트 부동소수점으로 풀기 · 손실 없음', quality: 'lossless' });
  // DSP 요청이 있으면 그 단계들(다운믹스, 볼륨 레벨링, 헤드룸, 필터들)을 엔진이 하는 순서대로.
  const plan = i.dsp ? planDsp(i.dsp, f, i.loudness, { caps: i.caps }) : null;
  const pick = (titles: string[]) => (plan ? plan.stages.filter((st) => titles.includes(st.title)) : []);
  steps.push(...pick(['다운믹스', '볼륨 레벨링']));
  if (!plan?.stages.some((st) => st.title === '볼륨 레벨링') && src && Math.abs(src.gainDb) >= 0.05) {
    steps.push({ title: '볼륨 레벨링', detail: `${fmtDb(src.gainDb)} · 64비트`, quality: 'enhanced' });
  }
  if (n.resampling) {
    const [a, b] = n.resampling;
    const precise = (i.dsp?.src?.f ?? 'precise') === 'precise';
    const bits = i.dsp?.src?.p ?? 28;
    steps.push({
      title: '샘플레이트 변환 · 64비트',
      detail: `${khz(a)}kHz → ${khz(b)}kHz · 다상 FIR ${bits}비트(${Math.round(bits * 6.02)}dB), ${precise ? '정밀' : '부드럽게'} · 선형 위상`,
      quality: 'high',
    });
  }
  if (plan) {
    const skip = new Set(['비트 깊이 변환', '다운믹스', '볼륨 레벨링', '샘플레이트 변환']);
    steps.push(...plan.stages.filter((st) => !skip.has(st.title)));
  }
  if (i.muted || i.volume < 1) {
    steps.push({
      title: '볼륨',
      detail: i.muted ? '음소거' : `${fmtDb(20 * Math.log10(Math.max(i.volume, 1e-4)))} · 64비트 디지털 볼륨`,
      quality: 'enhanced',
    });
  }
  const out = n.output;
  if (out) {
    const bits = src?.bits ?? (f.lossless ? f.bitDepth : undefined);
    // 16/24비트 원본을 손대지 않고 f32·i24·i32로 내보내면 값이 그대로 간다.
    const exact =
      (out.format === 'f64' || out.format === 'f32' || out.format === 'i32' || (out.format === 'i24' && (bits ?? 32) <= 24) || (out.format === 'i16' && (bits ?? 32) <= 16)) &&
      !n.resampling &&
      !src?.dsdRate &&
      !i.muted &&
      i.volume >= 1;
    if (out.format !== 'f64') {
      steps.push({
        title: '비트 깊이 변환',
        detail: `64비트 → ${FORMAT_LABEL[out.format]}${out.format.startsWith('i') && !exact ? ' · TPDF 디더' : exact ? ' · 값이 그대로 갑니다' : ''}`,
        quality: exact ? 'lossless' : 'high',
      });
    }
    steps.push({
      title: `${out.device}`,
      detail: `${out.api} · ${khz(out.rate)}kHz · ${out.channels}채널${out.mixer ? ' · OS 믹서를 지나므로 비트퍼펙트를 보장할 수 없습니다' : ' · 장치를 혼자 씁니다'}`,
      quality: out.mixer ? 'high' : 'lossless',
    });
  }
  return steps;
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
  if (i.native) {
    const steps = nativePath(i, i.native);
    return { quality: worst(steps), steps };
  }
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
