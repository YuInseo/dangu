/**
 * AutoEQ / Equalizer APO의 "ParametricEQ.txt"를 읽는다.
 *
 *   Preamp: -6.2 dB
 *   Filter 1: ON LSC Fc 105 Hz Gain 5.5 dB Q 0.70
 *   Filter 2: ON PK Fc 2300 Hz Gain -3.1 dB Q 1.41
 *
 * 헤드폰 측정 보정값을 사람 손으로 옮겨 적지 않아도 되게.
 */
import type { Band, BandType } from './types';

const TYPES: Record<string, BandType> = {
  PK: 'peak',
  PEQ: 'peak',
  LS: 'lowshelf',
  LSC: 'lowshelf',
  LSQ: 'lowshelf',
  HS: 'highshelf',
  HSC: 'highshelf',
  HSQ: 'highshelf',
  LP: 'lowpass',
  LPQ: 'lowpass',
  HP: 'highpass',
  HPQ: 'highpass',
  BP: 'bandpass',
  NO: 'notch',
};

export interface ParsedEq {
  preamp: number;
  bands: Band[];
  skipped: number;
}

export function parseAutoEq(text: string): ParsedEq {
  let preamp = 0;
  const bands: Band[] = [];
  let skipped = 0;
  for (const raw of text.split(/\r?\n/)) {
    const line = raw.trim();
    if (!line || line.startsWith('#')) continue;
    const pre = /^preamp\s*:\s*([-+]?\d+(?:\.\d+)?)\s*db/i.exec(line);
    if (pre) {
      preamp = Number(pre[1]);
      continue;
    }
    const m = /^filter\s*\d*\s*:\s*(on|off)\s+([a-z]+)\s+fc\s+([\d.]+)\s*hz(?:\s+gain\s+([-+]?[\d.]+)\s*db)?(?:\s+q\s+([\d.]+))?/i.exec(
      line,
    );
    if (!m) {
      if (/^filter/i.test(line)) skipped++;
      continue;
    }
    const type = TYPES[m[2]!.toUpperCase()];
    if (!type) {
      skipped++;
      continue;
    }
    bands.push({
      type,
      freq: Number(m[3]),
      gain: m[4] ? Number(m[4]) : 0,
      // 셸프의 Q가 없으면 AutoEQ 기본값 0.7, 나머지는 버터워스 0.707
      q: m[5] ? Number(m[5]) : 0.707,
      on: m[1]!.toLowerCase() === 'on',
    });
  }
  return { preamp, bands, skipped };
}

export function formatAutoEq(preamp: number, bands: Band[]): string {
  const code: Record<BandType, string> = {
    peak: 'PK',
    lowshelf: 'LSC',
    highshelf: 'HSC',
    lowpass: 'LPQ',
    highpass: 'HPQ',
    bandpass: 'BP',
    notch: 'NO',
  };
  const lines = [`Preamp: ${preamp.toFixed(1)} dB`];
  bands.forEach((b, i) =>
    lines.push(
      `Filter ${i + 1}: ${b.on ? 'ON' : 'OFF'} ${code[b.type]} Fc ${Math.round(b.freq)} Hz Gain ${b.gain.toFixed(1)} dB Q ${b.q.toFixed(2)}`,
    ),
  );
  return lines.join('\n');
}
