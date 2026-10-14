'use client';

/**
 * 이 기기에서의 화면 취향과 재생 동작. 룬의 "설정 → 일반 / 재생 동작"에 해당한다.
 * 서버로 가지 않고 브라우저에만 남는다.
 */
import { useSyncExternalStore } from 'react';
import { readJSON, writeJSON } from './player/storage';

export interface Prefs {
  /** 곡 목록에서 곡을 눌렀을 때 */
  trackClick: 'playFrom' | 'playNow' | 'next' | 'queue';
  /** 앨범 카드의 재생 버튼 */
  cardPlay: 'play' | 'shuffle';
  /** 표지 귀퉁이의 Hi-Res 표시 */
  hiresMark: boolean;
  /** 곡 목록의 포맷 칸: 앨범과 다를 때만 / 늘 / 끔 */
  formatColumn: 'diff' | 'always' | 'never';
  /** 진행 막대를 파형으로 */
  waveform: boolean;
  /** 볼륨 옆에 dB 표시 */
  volumeDb: boolean;
  /** 홈의 "오늘의 발견" */
  homeDiscover: boolean;
}

export const DEFAULT_PREFS: Prefs = {
  trackClick: 'playFrom',
  cardPlay: 'play',
  hiresMark: true,
  formatColumn: 'diff',
  waveform: true,
  volumeDb: true,
  homeDiscover: true,
};

const KEY = 'sori.prefs';
let cache: Prefs | null = null;
const listeners = new Set<() => void>();

export function getPrefs(): Prefs {
  cache ??= { ...DEFAULT_PREFS, ...(readJSON<Partial<Prefs>>(KEY) ?? {}) };
  return cache;
}

export function setPref<K extends keyof Prefs>(key: K, value: Prefs[K]) {
  cache = { ...getPrefs(), [key]: value };
  writeJSON(KEY, cache);
  for (const l of listeners) l();
}

function subscribe(l: () => void) {
  listeners.add(l);
  const onStorage = (e: StorageEvent) => {
    if (e.key === KEY) {
      cache = null;
      l();
    }
  };
  window.addEventListener('storage', onStorage);
  return () => {
    listeners.delete(l);
    window.removeEventListener('storage', onStorage);
  };
}

export function usePrefs(): Prefs {
  return useSyncExternalStore(subscribe, getPrefs, () => DEFAULT_PREFS);
}
