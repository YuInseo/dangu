/**
 * 바이쿼드 필터의 계수와 주파수 응답.
 *
 * ffmpeg의 equalizer/lowshelf/highshelf/lowpass/highpass/bandpass/bandreject와 똑같은
 * RBJ "Audio EQ Cookbook" 공식을 쓴다. 그래서 화면의 곡선이 서버가 실제로 거는 필터와 맞는다.
 */
import type { Band } from './types';

export interface Coeffs {
  b0: number;
  b1: number;
  b2: number;
  a0: number;
  a1: number;
  a2: number;
}

export function coeffs(band: Band, fs: number): Coeffs {
  const f = Math.min(Math.max(band.freq, 1), fs / 2 - 1);
  const w0 = (2 * Math.PI * f) / fs;
  const cos = Math.cos(w0);
  const sin = Math.sin(w0);
  const q = Math.max(band.q, 0.01);
  const alpha = sin / (2 * q);
  const A = Math.pow(10, band.gain / 40);
  switch (band.type) {
    case 'peak':
      return { b0: 1 + alpha * A, b1: -2 * cos, b2: 1 - alpha * A, a0: 1 + alpha / A, a1: -2 * cos, a2: 1 - alpha / A };
    case 'lowshelf': {
      const beta = 2 * Math.sqrt(A) * alpha;
      return {
        b0: A * (A + 1 - (A - 1) * cos + beta),
        b1: 2 * A * (A - 1 - (A + 1) * cos),
        b2: A * (A + 1 - (A - 1) * cos - beta),
        a0: A + 1 + (A - 1) * cos + beta,
        a1: -2 * (A - 1 + (A + 1) * cos),
        a2: A + 1 + (A - 1) * cos - beta,
      };
    }
    case 'highshelf': {
      const beta = 2 * Math.sqrt(A) * alpha;
      return {
        b0: A * (A + 1 + (A - 1) * cos + beta),
        b1: -2 * A * (A - 1 + (A + 1) * cos),
        b2: A * (A + 1 + (A - 1) * cos - beta),
        a0: A + 1 - (A - 1) * cos + beta,
        a1: 2 * (A - 1 - (A + 1) * cos),
        a2: A + 1 - (A - 1) * cos - beta,
      };
    }
    case 'lowpass':
      return { b0: (1 - cos) / 2, b1: 1 - cos, b2: (1 - cos) / 2, a0: 1 + alpha, a1: -2 * cos, a2: 1 - alpha };
    case 'highpass':
      return { b0: (1 + cos) / 2, b1: -(1 + cos), b2: (1 + cos) / 2, a0: 1 + alpha, a1: -2 * cos, a2: 1 - alpha };
    case 'bandpass':
      return { b0: alpha, b1: 0, b2: -alpha, a0: 1 + alpha, a1: -2 * cos, a2: 1 - alpha };
    case 'notch':
      return { b0: 1, b1: -2 * cos, b2: 1, a0: 1 + alpha, a1: -2 * cos, a2: 1 - alpha };
  }
}

/** 주파수 f에서의 크기, dB. */
export function magnitudeDb(c: Coeffs, f: number, fs: number): number {
  const w = (2 * Math.PI * f) / fs;
  const cos1 = Math.cos(w);
  const sin1 = Math.sin(w);
  const cos2 = Math.cos(2 * w);
  const sin2 = Math.sin(2 * w);
  const nr = c.b0 + c.b1 * cos1 + c.b2 * cos2;
  const ni = -(c.b1 * sin1 + c.b2 * sin2);
  const dr = c.a0 + c.a1 * cos1 + c.a2 * cos2;
  const di = -(c.a1 * sin1 + c.a2 * sin2);
  const num = nr * nr + ni * ni;
  const den = dr * dr + di * di;
  return 10 * Math.log10(Math.max(num, 1e-30) / Math.max(den, 1e-30));
}

/** 20Hz–20kHz를 로그 간격으로. */
export function logFrequencies(count = 256, lo = 20, hi = 20000): number[] {
  const a = Math.log10(lo);
  const b = Math.log10(hi);
  return Array.from({ length: count }, (_, i) => Math.pow(10, a + ((b - a) * i) / (count - 1)));
}

/** 밴드들을 이어 붙인 전체 응답(+프리앰프), dB. */
export function response(bands: Band[], freqs: number[], fs = 48000, preamp = 0): number[] {
  const cs = bands.filter((b) => b.on).map((b) => coeffs(b, fs));
  return freqs.map((f) => preamp + cs.reduce((s, c) => s + magnitudeDb(c, f, fs), 0));
}

/** 가장 크게 올라가는 곳. 헤드룸을 얼마나 확보해야 하는지의 근거. */
export function maxBoost(bands: Band[], preamp = 0, fs = 48000): number {
  const curve = response(bands, logFrequencies(400, 10, Math.min(22000, fs / 2 - 1)), fs, preamp);
  return Math.max(0, ...curve);
}

export const BAND_LABEL: Record<Band['type'], string> = {
  peak: '피크/딥',
  lowshelf: '저역 셸프',
  highshelf: '고역 셸프',
  lowpass: '로우패스',
  highpass: '하이패스',
  bandpass: '밴드패스',
  notch: '노치',
};

/** 게인이 의미 있는 종류인지. 통과·차단 필터는 게인이 없다. */
export function hasGain(t: Band['type']): boolean {
  return t === 'peak' || t === 'lowshelf' || t === 'highshelf';
}
