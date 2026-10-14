'use client';

import Link from 'next/link';
import { useEffect, useState } from 'react';
import { readRecent, type RecentEntry } from '@/lib/player/storage';
import { Cover } from './cover';
import { Section } from './section';

/** 이 브라우저에서 최근에 튼 앨범과 공연. 기기마다 따로다. */
export function RecentlyPlayed() {
  const [items, setItems] = useState<RecentEntry[]>([]);
  useEffect(() => {
    const load = () => setItems(readRecent());
    load();
    window.addEventListener('sori:recent', load);
    return () => window.removeEventListener('sori:recent', load);
  }, []);
  if (!items.length) return null;
  return (
    <Section title="최근 재생">
      <div className="row-scroll">
        {items.slice(0, 16).map((r) => (
          <Link
            key={`${r.kind}:${r.id}`}
            href={r.kind === 'album' ? `/albums/${r.id}` : `/live/${encodeURIComponent(r.id)}`}
            className="card"
            prefetch={false}
          >
            <div className="card-art">
              <Cover src={r.cover?.sm} color={r.cover?.color} alt="" seed={r.seed} title={r.title} subtitle={r.artist} />
            </div>
            <div className="card-title">{r.title}</div>
            <div className="card-sub">{r.kind === 'live' ? `라이브 · ${r.artist}` : r.artist}</div>
          </Link>
        ))}
      </div>
    </Section>
  );
}
