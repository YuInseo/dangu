import { useMemo } from 'react';
import Link from '@/compat/link';
import { useSearchParams } from '@/compat/navigation';
import { useApi, useTitle } from '@/api';
import { AlbumGrid } from '@/components/album-card';
import { Heart } from '@/components/icons';
import { PlayButtons } from '@/components/play-buttons';
import { TrackTable } from '@/components/track-table';
import type { AlbumCard, Track } from '@/lib/library/types';
import { fromLibrary } from '@/lib/player/tracks';
import { count, span } from '@/lib/time';
import { Failed, Loading } from './common';

/** 좋아요한 곡과 앨범. 곡이 먼저, 앨범은 옆 칸에. */
export function FavoritesPage() {
  useTitle('좋아요');
  const sp = useSearchParams();
  const tab = sp.get('tab') === 'albums' ? 'albums' : 'tracks';
  const { data, error } = useApi<{ tracks: Track[]; albums: AlbumCard[] }>('/api/favorites');
  const tracks = useMemo(() => data?.tracks.map(fromLibrary) ?? [], [data]);
  if (error) return <Failed error={error} />;
  if (!data) return <Loading />;
  const duration = tracks.reduce((s, t) => s + t.duration, 0);

  return (
    <div className="detail">
      <header className="detail-head">
        <div className="detail-art">
          <div className="fav-art" aria-hidden>
            <Heart fill="currentColor" strokeWidth={0} />
          </div>
        </div>
        <div className="detail-info">
          <div className="eyebrow">보관함</div>
          <h1 className="detail-title">좋아요</h1>
          <div className="detail-meta">
            {[`${count(tracks.length)}곡`, duration ? span(duration) : null, data.albums.length ? `앨범 ${count(data.albums.length)}장` : null]
              .filter(Boolean)
              .join(' · ')}
          </div>
          <div className="detail-gap" />
          <PlayButtons tracks={tracks} label="좋아요" />
        </div>
      </header>

      <div className="toolbar">
        <div className="seg" role="tablist" aria-label="보기">
          <Link href="/favorites" className={`seg-item${tab === 'tracks' ? ' is-active' : ''}`} replace scroll={false}>
            곡 {count(tracks.length)}
          </Link>
          <Link href="/favorites?tab=albums" className={`seg-item${tab === 'albums' ? ' is-active' : ''}`} replace scroll={false}>
            앨범 {count(data.albums.length)}
          </Link>
        </div>
      </div>

      {tab === 'tracks' ? (
        tracks.length ? (
          <TrackTable tracks={tracks} variant="list" />
        ) : (
          <CollectionEmpty title="아직 좋아요한 곡이 없습니다">
            곡의 ⋯ 메뉴나 지금 재생 중 화면에서 ♥를 누르면 여기에 모입니다.
          </CollectionEmpty>
        )
      ) : data.albums.length ? (
        <AlbumGrid albums={data.albums} showYear />
      ) : (
        <CollectionEmpty title="아직 좋아요한 앨범이 없습니다">앨범 화면의 ♥를 누르면 여기에 모입니다.</CollectionEmpty>
      )}
    </div>
  );
}

export function CollectionEmpty({ title, children }: { title: string; children?: React.ReactNode }) {
  return (
    <div className="coll-empty">
      <div className="coll-empty-title">{title}</div>
      {children && <p>{children}</p>}
    </div>
  );
}
