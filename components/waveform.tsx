'use client';

import { useEffect, useState } from 'react';
import type { PlayableTrack } from '@/lib/player/types';

const cache = new Map<string, Uint8Array | null>();

/** 분석이 끝난 라이브러리 곡의 파형. 라이브 아카이브 곡이나 분석 전이면 null. */
export function useWaveform(track: PlayableTrack | undefined): Uint8Array | null {
  const id = track?.source === 'library' ? track.key.slice(4) : null;
  const [wave, setWave] = useState<Uint8Array | null>(() => (id ? (cache.get(id) ?? null) : null));

  useEffect(() => {
    if (!id) {
      setWave(null);
      return;
    }
    if (cache.has(id)) {
      setWave(cache.get(id) ?? null);
      return;
    }
    setWave(null);
    let alive = true;
    fetch(`/api/waveform/${id}`)
      .then((r) => (r.ok ? r.json() : null))
      .then((j: { w: string } | null) => {
        const w = j ? Uint8Array.from(atob(j.w), (c) => c.charCodeAt(0)) : null;
        // 아직 분석 전이면 기억하지 않는다 — 다음에 다시 물어본다
        if (w) cache.set(id, w);
        if (alive) setWave(w);
      })
      .catch(() => {});
    return () => {
      alive = false;
    };
  }, [id]);

  return wave;
}

/** 막대 모양 파형. 재생한 곳은 칠하고, 나머지는 흐리게. */
export function WaveBars({ wave }: { wave: Uint8Array }) {
  const n = wave.length;
  const bars = Array.from(wave, (v, i) => {
    const h = Math.max(6, (v / 255) * 100);
    return <rect key={i} x={i + 0.15} y={(100 - h) / 2} width={0.7} height={h} rx={0.35} />;
  });
  return (
    <svg className="wave-svg" viewBox={`0 0 ${n} 100`} preserveAspectRatio="none" aria-hidden>
      {bars}
    </svg>
  );
}
