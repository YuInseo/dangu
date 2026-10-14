/**
 * 샘플레이트 변환의 목표 정하기. 룬의 모드를 브라우저에 맞게 옮겼다.
 *
 * - match:  출력 장치의 샘플레이트에 맞춘다. 브라우저가 따로 리샘플링하지 않게 하는 길이라 기본값.
 * - max2:   같은 계열(44.1k/48k)을 지키며 2배씩 올린다 — 룬의 "Max PCM rate (Power of 2)".
 * - compat: 원본이 장치 최대치를 넘을 때만 같은 계열로 낮춘다 — "For Compatibility Only".
 * - custom: 원본 샘플레이트마다 목표를 직접 정한다.
 */
import type { SrcConfig } from './types';

export function srcTarget(cfg: SrcConfig, source: number | undefined, device: number | null): number | null {
  if (!cfg.enabled || !source) return null;
  let r: number | null = null;
  switch (cfg.mode) {
    case 'match':
      r = device;
      break;
    case 'max2': {
      r = source;
      while (r * 2 <= cfg.maxRate) r *= 2;
      while (r > cfg.maxRate && r % 2 === 0) r /= 2;
      break;
    }
    case 'compat': {
      if (source > cfg.maxRate) {
        r = source;
        while (r > cfg.maxRate && r % 2 === 0) r /= 2;
      }
      break;
    }
    case 'custom':
      r = cfg.custom[String(source)] ?? null;
      break;
  }
  if (!r || r === source || r < 8000 || r > 768000) return null;
  return r;
}

export const SRC_MODE_LABEL: Record<SrcConfig['mode'], string> = {
  match: '출력 장치에 맞춤',
  max2: '최대 PCM (2의 거듭제곱)',
  compat: '호환성 전용',
  custom: '사용자 지정',
};

export const SRC_FILTER_LABEL: Record<SrcConfig['filter'], string> = {
  precise: '정밀 · 선형 위상',
  smooth: '부드럽게 · 선형 위상',
};
