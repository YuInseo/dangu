import { notFound } from 'next/navigation';
import type { Metadata } from 'next';
import { AlbumGrid } from '@/components/album-card';
import { Backdrop } from '@/components/backdrop';
import { Cover } from '@/components/cover';
import { PlayButtons } from '@/components/play-buttons';
import { Section } from '@/components/section';
import { TrackTable } from '@/components/track-table';
import { toCard } from '@/lib/library/build';
import { getLibrary, loadLibrary } from '@/lib/library/store';
import { coverUrls, fromLibrary } from '@/lib/player/tracks';
import { span } from '@/lib/time';

type Props = { params: Promise<{ id: string }> };

export async function generateMetadata({ params }: Props): Promise<Metadata> {
  const { id } = await params;
  const artist = (await getLibrary()).artists.get(id);
  return { title: artist?.name ?? '아티스트' };
}

export default async function ArtistPage({ params }: Props) {
  const { id } = await params;
  const lib = await loadLibrary();
  const artist = lib.artists.get(id);
  if (!artist) notFound();

  const albums = artist.albumIds.map((a) => lib.albums.get(a)!);
  // 연도 순으로 앨범을 이어 붙인 것이 "이 아티스트 전부 재생".
  const own = [...albums]
    .sort((a, b) => (a.year ?? 0) - (b.year ?? 0))
    .flatMap((a) => a.trackIds.map((t) => fromLibrary(lib.views.get(t)!)));
  const appears = artist.appearsOn.map((t) => fromLibrary(lib.views.get(t)!));
  const all = [...own, ...appears];
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
          <AlbumGrid albums={albums.map(toCard)} showYear />
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
