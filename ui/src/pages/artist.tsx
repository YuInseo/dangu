import { useMemo } from 'react';
import { useParams } from 'react-router';
import { useApi, useTitle } from '@/api';
import { AlbumGrid } from '@/components/album-card';
import { Backdrop } from '@/components/backdrop';
import { Cover } from '@/components/cover';
import { PlayButtons } from '@/components/play-buttons';
import { Section } from '@/components/section';
import { TrackTable } from '@/components/track-table';
import type { AlbumCard, Artist, Track } from '@/lib/library/types';
import { coverUrls, fromLibrary } from '@/lib/player/tracks';
import { span } from '@/lib/time';
import { Failed, Loading } from './common';

interface ArtistData {
  artist: Artist;
  albums: AlbumCard[];
  own: Track[];
  appears: Track[];
}

export function ArtistPage() {
  const { id = '' } = useParams();
  const { data, error } = useApi<ArtistData>(`/api/artists/${encodeURIComponent(id)}`);
  useTitle(data?.artist.name ?? '아티스트');
  const { all, appears } = useMemo(() => {
    const own = data?.own.map(fromLibrary) ?? [];
    const appears = data?.appears.map(fromLibrary) ?? [];
    return { all: [...own, ...appears], appears };
  }, [data]);
  if (error) return <Failed error={error} />;
  if (!data) return <Loading />;
  const { artist, albums } = data;
  const cover = artist.coverAlbumId ? coverUrls(artist.coverAlbumId, artist.cover) : undefined;
  const duration = all.reduce((s, t) => s + t.duration, 0);

  return (
    <div className="detail">
      <Backdrop src={cover?.sm} color={cover?.color} />
      <header className="detail-head detail-head-artist">
        <div className="detail-art detail-art-round">
          <Cover src={cover?.lg} color={cover?.color} alt={artist.name} seed={artist.id} title={artist.name} round size="sm" priority />
          {!cover && <span className="initial initial-lg">{artist.name.trim().charAt(0).toUpperCase()}</span>}
        </div>
        <div className="detail-info">
          <div className="eyebrow">아티스트</div>
          <h1 className="detail-title">{artist.name}</h1>
          <div className="detail-meta">
            {[albums.length ? `앨범 ${albums.length}장` : null, `${all.length}곡`, span(duration)].filter(Boolean).join(' · ')}
          </div>
          <PlayButtons tracks={all} />
        </div>
      </header>

      {albums.length > 0 && (
        <Section title="앨범">
          <AlbumGrid albums={albums} showYear />
        </Section>
      )}
      {appears.length > 0 && (
        <Section title="참여한 곡">
          <TrackTable tracks={appears} variant="list" />
        </Section>
      )}
    </div>
  );
}
