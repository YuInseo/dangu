import Link from '@/compat/link';
import type { Artist } from '@/lib/library/types';
import { coverUrls } from '@/lib/player/tracks';
import { Cover } from './cover';

export function ArtistCard({ artist }: { artist: Artist }) {
  const cover = artist.coverAlbumId ? coverUrls(artist.coverAlbumId, artist.cover) : undefined;
  const n = artist.albumIds.length;
  return (
    <Link href={`/artists/${artist.id}`} className="card card-artist" prefetch={false}>
      <div className="card-art">
        <Cover src={cover?.sm} color={cover?.color} alt="" seed={artist.id} title={artist.name} round size="sm" />
        {!cover && <span className="initial">{artist.name.trim().charAt(0).toUpperCase()}</span>}
      </div>
      <div className="card-title">{artist.name}</div>
      <div className="card-sub">{n ? `앨범 ${n}장` : `참여 ${artist.appearsOn.length}곡`}</div>
    </Link>
  );
}

export function ArtistGrid({ artists }: { artists: Artist[] }) {
  return (
    <div className="grid grid-artists">
      {artists.map((a) => (
        <ArtistCard key={a.id} artist={a} />
      ))}
    </div>
  );
}
