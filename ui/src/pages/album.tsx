import { useMemo } from 'react';
import { useParams } from 'react-router';
import Link from '@/compat/link';
import { useApi, useTitle } from '@/api';
import { AlbumRow } from '@/components/album-card';
import { Backdrop } from '@/components/backdrop';
import { Cover } from '@/components/cover';
import { PlayButtons } from '@/components/play-buttons';
import { QualityBadge } from '@/components/quality-badge';
import { Section } from '@/components/section';
import { TrackTable } from '@/components/track-table';
import type { Album, AlbumCard, Track } from '@/lib/library/types';
import { coverUrls, fromLibrary } from '@/lib/player/tracks';
import { formatLabel } from '@/lib/quality';
import { span } from '@/lib/time';
import { Failed, Loading } from './common';

interface AlbumData {
  album: Album;
  tracks: Track[];
  more: AlbumCard[];
  genres: { id: string; name: string }[];
}

export function AlbumPage() {
  const { id = '' } = useParams();
  const { data, error } = useApi<AlbumData>(`/api/albums/${encodeURIComponent(id)}`);
  useTitle(data ? `${data.album.title} — ${data.album.artist}` : '앨범');
  const tracks = useMemo(() => data?.tracks.map(fromLibrary) ?? [], [data]);
  if (error) return <Failed error={error} />;
  if (!data) return <Loading />;
  const { album, more, genres } = data;
  const cover = coverUrls(album.id, album.cover);

  return (
    <div className="detail">
      <Backdrop src={cover?.sm} color={cover?.color} />
      <header className="detail-head">
        <div className="detail-art">
          <Cover
            src={cover?.lg}
            color={cover?.color}
            alt={album.title}
            seed={album.id}
            title={album.title}
            subtitle={album.artist}
            size="lg"
            priority
          />
        </div>
        <div className="detail-info">
          <div className="eyebrow">앨범</div>
          <h1 className="detail-title">{album.title}</h1>
          <Link href={`/artists/${album.artistId}`} className="detail-artist">
            {album.artist}
          </Link>
          <div className="detail-meta">
            {[album.year, `${tracks.length}곡`, span(album.duration), album.discCount > 1 ? `디스크 ${album.discCount}장` : null]
              .filter(Boolean)
              .join(' · ')}
          </div>
          <div className="detail-tags">
            <QualityBadge format={album.format} />
            {genres.map((g) => (
              <Link key={g.id} href={`/genres/${g.id}`} className="tag">
                {g.name}
              </Link>
            ))}
          </div>
          <PlayButtons tracks={tracks} albumId={album.id} label={album.title} />
        </div>
      </header>

      <TrackTable
        tracks={tracks}
        variant="album"
        albumArtist={album.artist}
        discs={album.discCount > 1}
        albumFormat={formatLabel(album.format)}
      />

      {more.length > 0 && (
        <Section title={`${album.artist}의 다른 앨범`} href={`/artists/${album.artistId}`}>
          <AlbumRow albums={more} showYear />
        </Section>
      )}
    </div>
  );
}
