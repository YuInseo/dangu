/**
 * 볼륨 레벨링. 룬과 같은 규칙:
 * - 목표 라우드니스(기본 -14 LUFS)에서 곡의 라우드니스를 뺀 만큼 게인을 준다.
 * - 올리는 게인은 트루 피크를 넘지 않게 제한한다 — 레벨링 때문에 클리핑이 나지 않는다.
 * - 라우드니스를 모르는 곡에는 고정 게인(기본 -5dB)을 준다. 레벨링은 대부분 소리를 줄이므로,
 *   분석된 곡과 안 된 곡 사이에서 갑자기 커지지 않게 하려는 것이다.
 */
import type { Loudness } from './types';

export interface LevelingResult {
  gain: number;
  basis: 'track' | 'album' | 'unknown';
  limited: boolean;
  lufs?: number;
}

export function levelingGain(l: Loudness | undefined, mode: 'track' | 'album', target: number, unknown: number): LevelingResult {
  const v = mode === 'album' ? (l?.album ?? l?.track) : l?.track;
  const basis = mode === 'album' && l?.album ? 'album' : 'track';
  // 무음에 가까운 곡(-70 LUFS 게이트)은 모르는 곡으로 친다. 아니면 +50dB를 주려 든다.
  if (!v || !Number.isFinite(v.lufs) || v.lufs <= -69) return { gain: round(unknown), basis: 'unknown', limited: false };
  let gain = target - v.lufs;
  let limited = false;
  if (gain > 0 && Number.isFinite(v.peak)) {
    const ceiling = -v.peak;
    if (gain > ceiling) {
      gain = Math.max(0, ceiling);
      limited = true;
    }
  }
  return { gain: round(gain), basis, limited, lufs: v.lufs };
}

function round(x: number) {
  return Math.round(x * 10) / 10;
}

/** 앨범 라우드니스: 곡 길이로 가중한 에너지 평균. 정식 앨범 게이팅의 근사값이다. */
export function albumLoudness(tracks: { lufs: number; peak: number; duration: number }[]) {
  const valid = tracks.filter((t) => Number.isFinite(t.lufs) && t.lufs > -69 && t.duration > 0);
  if (!valid.length) return undefined;
  const total = valid.reduce((s, t) => s + t.duration, 0);
  const energy = valid.reduce((s, t) => s + t.duration * Math.pow(10, t.lufs / 10), 0) / total;
  return {
    lufs: Math.round(10 * Math.log10(energy) * 10) / 10,
    peak: Math.max(...valid.map((t) => t.peak)),
  };
}
