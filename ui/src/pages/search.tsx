import { useMemo } from 'react';
import Link from '@/compat/link';
import { useSearchParams } from '@/compat/navigation';
import { useApi, useTitle } from '@/api';
import { AlbumGrid } from '@/components/album-card';
import { ArtistGrid } from '@/components/artist-card';
import { Radio, Search } from '@/components/icons';
import { PageHeader, Section } from '@/components/section';
import { TrackTable } from '@/components/track-table';
import type { AlbumCard, Artist, Track } from '@/lib/library/types';
import { fromLibrary } from '@/lib/player/tracks';

export function SearchPage() {
  const q = (useSearchParams().get('q') ?? '').trim();
  useTitle(q ? `“${q}”` : '검색');
  const { data } = useApi<{ artists: Artist[]; albums: AlbumCard[]; tracks: Track[] }>(q ? `/api/search?q=${encodeURIComponent(q)}` : null);
  const tracks = useMemo(() => data?.tracks.map(fromLibrary) ?? [], [data]);
  const r = data ?? { artists: [], albums: [], tracks: [] };
  const nothing = q && data && !r.artists.length && !r.albums.length && !r.tracks.length;

  return (
    <>
      <PageHeader title={q ? `“${q}”` : '검색'} eyebrow={q ? '검색 결과' : undefined} />
      {!q && (
        <div className="hint">
          <Search size={18} />
          <span>위의 검색창에 아티스트, 앨범, 곡 이름을 치세요. 어디서든 / 를 누르면 검색창으로 갑니다.</span>
        </div>
      )}
      {nothing && <p className="muted pad">내 라이브러리에서 찾지 못했습니다.</p>}
      {r.artists.length > 0 && (
        <Section title="아티스트">
          <ArtistGrid artists={r.artists} />
        </Section>
      )}
      {r.albums.length > 0 && (
        <Section title="앨범">
          <AlbumGrid albums={r.albums} />
        </Section>
      )}
      {tracks.length > 0 && (
        <Section title="곡">
          <TrackTable tracks={tracks} variant="list" />
        </Section>
      )}
      {q && (
        <Link href={`/live?q=${encodeURIComponent(q)}`} className="live-teaser">
          <div className="live-teaser-icon">
            <Radio size={22} />
          </div>
          <div>
            <div className="live-teaser-title">라이브 아카이브에서 “{q}” 찾기</div>
            <div className="live-teaser-sub">공연 실황을 무손실로. 라이브러리에 없는 아티스트도 있을 수 있어요.</div>
          </div>
        </Link>
      )}
    </>
  );
}
