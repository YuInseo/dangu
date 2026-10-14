'use client';

import { useRouter } from '@/compat/navigation';
import { useState } from 'react';
import { FEATURED_BANDS } from '@/lib/live/shared';
import { Search } from './icons';

export function LiveSearch({ initial }: { initial: string }) {
  const router = useRouter();
  const [q, setQ] = useState(initial);
  const go = (v: string) => router.push(v.trim() ? `/live?q=${encodeURIComponent(v.trim())}` : '/live');
  return (
    <div className="live-search">
      <form
        className="filter filter-lg"
        onSubmit={(e) => {
          e.preventDefault();
          go(q);
        }}
      >
        <Search size={18} />
        <input value={q} onChange={(e) => setQ(e.target.value)} placeholder="밴드, 공연장 이름 (영어)" aria-label="라이브 아카이브 검색" />
        <button type="submit" className="btn btn-sm">
          찾기
        </button>
      </form>
      <div className="chips">
        {FEATURED_BANDS.map((b) => (
          <button
            key={b}
            type="button"
            className={`chip${b === initial ? ' is-active' : ''}`}
            onClick={() => {
              setQ(b);
              go(b);
            }}
          >
            {b}
          </button>
        ))}
      </div>
    </div>
  );
}
