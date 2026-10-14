import type { Band, DspConfig, DspFilter, FilterType } from './types';

export const CROSSFEED_PRESETS = {
  // libbs2b의 세 프리셋. 룬도 같은 라이브러리(Bauer stereo-to-binaural)를 쓴다.
  default: { fcut: 700, feed: 4.5, label: '기본 (Bauer)' },
  cmoy: { fcut: 700, feed: 6.0, label: 'Chu Moy' },
  jmeier: { fcut: 650, feed: 9.5, label: 'Jan Meier' },
} as const;

export const FILTER_LABEL: Record<FilterType, string> = {
  crossfeed: '크로스피드',
  peq: '파라메트릭 EQ',
  procedural: '프로시저럴 EQ',
  headphone: '헤드폰 EQ',
  convolution: '컨볼루션',
};

export const FILTER_BLURB: Record<FilterType, string> = {
  crossfeed: '헤드폰에서 왼쪽·오른쪽 소리를 조금씩 섞어 스피커로 듣는 것처럼 만듭니다.',
  peq: '주파수·게인·Q로 밴드를 하나하나 정하는 이퀄라이저.',
  procedural: '채널별 볼륨·지연·위상 반전·믹스를 순서대로 쌓는 고급 도구. 디지털 크로스오버, 수동 룸 보정에 씁니다.',
  headphone: '헤드폰 측정 보정값(AutoEQ 형식)을 붙여 넣어 적용합니다.',
  convolution: '임펄스 응답 파일(WAV·FLAC·AIFF)로 룸 보정이나 헤드폰 보정을 합니다.',
};

let n = 0;
export function newId(): string {
  return `${Date.now().toString(36)}${(n++).toString(36)}${Math.random().toString(36).slice(2, 5)}`;
}

export function defaultBand(freq = 1000): Band {
  return { type: 'peak', freq, gain: 0, q: 1, on: true };
}

export function newFilter(type: FilterType): DspFilter {
  const id = newId();
  switch (type) {
    case 'crossfeed':
      return { id, type, enabled: false, preset: 'default', fcut: 700, feed: 4.5 };
    case 'peq':
      return { id, type, enabled: false, preamp: 0, bands: [] };
    case 'procedural':
      return { id, type, enabled: false, steps: [] };
    case 'headphone':
      return { id, type, enabled: false, model: '', preamp: 0, bands: [] };
    case 'convolution':
      return { id, type, enabled: false, ir: null, gain: 0, normalize: true };
  }
}

/** 룬의 첫 화면처럼, 모든 필터가 꺼진 채로 한 줄씩 놓여 있다. */
export function defaultConfig(): DspConfig {
  return {
    v: 1,
    enabled: false,
    headroom: { enabled: true, gain: -3, clipIndicator: true },
    src: { enabled: false, mode: 'match', filter: 'precise', precision: 28, maxRate: 192000, custom: {} },
    leveling: { mode: 'off', target: -14, unknown: -5 },
    output: { bits: 24, dither: 'tpdf' },
    bypass: false,
    filters: [newFilter('crossfeed'), newFilter('peq'), newFilter('procedural'), newFilter('headphone'), newFilter('convolution')],
  };
}

export const STANDARD_RATES = [44100, 48000, 88200, 96000, 176400, 192000, 352800, 384000];
