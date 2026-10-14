import Link from 'next/link';
import { notFound } from 'next/navigation';
import type { Metadata } from 'next';
import { AlbumRow } from '@/components/album-card';
import { Backdrop } from '@/components/backdrop';
import { Cover } from '@/components/cover';
import { PlayButtons } from '@/components/play-buttons';
import { QualityBadge } from '@/components/quality-badge';
import { Section } from '@/components/section';
import { TrackTable } from '@/components/track-table';
import { toCard } from '@/lib/library/build';
import { genreId } from '@/lib/library/ids';
import { getLibrary, loadLibrary } from '@/lib/library/store';
import { coverUrls, fromLibrary } from '@/lib/player/tracks';
import { formatLabel } from '@/lib/quality';
import { span } from '@/lib/time';

type Props = { params: Promise<{ id: string }> };

export async function generateMetadata({ params }: Props): Promise<Metadata> {
  const { id } = await params;
  const album = (await getLibrary()).albums.get(id);
  return { title: album ? `${album.title} — ${album.artist}` : '앨범' };
}

export default async function AlbumPage({ params }: Props) {
  const { id } = await params;
  const lib = await loadLibrary();
  const album = lib.albums.get(id);
  if (!album) notFound();

  const tracks = album.trackIds.map((t) => fromLibrary(lib.views.get(t)!));
  const cover = coverUrls(album.id, album.cover);
  const more = (lib.artists.get(album.artistId)?.albumIds ?? []).filter((x) => x !== id).map((x) => toCard(lib.albums.get(x)!));

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
            {album.genres.map((g) => (
              <Link key={g} href={`/genres/${genreId(g)}`} className="tag">
                {g}
              </Link>
            ))}
          </div>
          <PlayButtons tracks={tracks} />
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
