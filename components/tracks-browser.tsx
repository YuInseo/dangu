'use client';

import { useEffect, useMemo, useRef, useState } from 'react';
import type { Track } from '@/lib/library/types';
import { fromLibrary } from '@/lib/player/tracks';
import { count } from '@/lib/time';
import { Search } from './icons';
import { TrackTable } from './track-table';

const PAGE = 200;

/**
 * 곡 전체. 수만 곡을 한 번에 받지 않고, 끝에 닿을 때마다 200곡씩 더 받는다.
 */
export function TracksBrowser({ initial, total: initialTotal }: { initial: Track[]; total: number }) {
  const [tracks, setTracks] = useState(initial);
  const [total, setTotal] = useState(initialTotal);
  const [q, setQ] = useState('');
  const [loading, setLoading] = useState(false);
  const sentinel = useRef<HTMLDivElement>(null);
  const req = useRef(0);

  const load = async (query: string, offset: number) => {
    const id = ++req.current;
    setLoading(true);
    try {
      const res = await fetch(`/api/tracks?offset=${offset}&limit=${PAGE}&q=${encodeURIComponent(query)}`);
      const json = (await res.json()) as { total: number; tracks: Track[] };
      if (id !== req.current) return;
      setTotal(json.total);
      setTracks((prev) => (offset === 0 ? json.tracks : [...prev, ...json.tracks]));
    } finally {
      if (id === req.current) setLoading(false);
    }
  };

  // 검색어가 바뀌면 처음부터
  const first = useRef(true);
  useEffect(() => {
    if (first.current) {
      first.current = false;
      return;
    }
    const t = setTimeout(() => void load(q, 0), 200);
    return () => clearTimeout(t);
  }, [q]);

  useEffect(() => {
    const el = sentinel.current;
    if (!el) return;
    const io = new IntersectionObserver(
      (entries) => {
        if (entries[0]?.isIntersecting && !loading && tracks.length < total) void load(q, tracks.length);
      },
      { rootMargin: '800px' },
    );
    io.observe(el);
    return () => io.disconnect();
  }, [loading, tracks.length, total, q]);

  const playable = useMemo(() => tracks.map(fromLibrary), [tracks]);

  return (
    <>
      <div className="toolbar">
        <label className="filter">
          <Search size={16} />
          <input value={q} onChange={(e) => setQ(e.target.value)} placeholder="곡 거르기" aria-label="곡 거르기" />
        </label>
        <span className="muted">{q ? `${count(total)}곡 일치` : ''}</span>
      </div>
      <TrackTable tracks={playable} variant="list" />
      <div ref={sentinel} className="sentinel">
        {loading ? '불러오는 중…' : tracks.length < total ? '' : tracks.length ? '' : '일치하는 곡이 없습니다.'}
      </div>
    </>
  );
}
