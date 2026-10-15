import Link from '@/compat/link';
import { useApi, useTitle } from '@/api';
import { ChevronRight, Disc3, Heart, Library, MicVocal, Tags } from '@/components/icons';
import { PlaylistArt } from '@/components/playlist-art';
import { PageHeader } from '@/components/section';
import { useLikedCount, type PlaylistSummary } from '@/lib/collections';
import { count, span } from '@/lib/time';
import { useLibraryInfo } from '@/library';
import { NewPlaylist } from './playlists';

/**
 * 보관함. 휴대폰 아래쪽 탭에서 오는 곳: 아티스트·앨범·곡·장르로 가는 길과, 좋아요·플레이리스트를 한 목록에.
 */
export function CollectionPage() {
  useTitle('보관함');
  const { summary } = useLibraryInfo();
  const likedTracks = useLikedCount('tracks');
  const likedAlbums = useLikedCount('albums');
  const { data } = useApi<PlaylistSummary[]>('/api/playlists');

  const browse = [
    { href: '/artists', label: '아티스트', icon: MicVocal, n: summary.artists },
    { href: '/albums', label: '앨범', icon: Disc3, n: summary.albums },
    { href: '/tracks', label: '곡', icon: Library, n: summary.tracks },
    { href: '/genres', label: '장르', icon: Tags, n: undefined },
  ];

  return (
    <div className="coll">
      <PageHeader title="보관함" />

      <div className="coll-browse">
        {browse.map(({ href, label, icon: Icon, n }) => (
          <Link key={href} href={href} className="coll-browse-item">
            <Icon size={20} />
            <span className="coll-browse-label">{label}</span>
            {n !== undefined && <span className="coll-browse-n">{count(n)}</span>}
          </Link>
        ))}
      </div>

      <div className="section-head coll-head">
        <h2 className="section-title">좋아요 · 플레이리스트</h2>
        <NewPlaylist compact />
      </div>

      <div className="coll-list">
        <Link href="/favorites" className="coll-row">
          <span className="coll-row-art">
            <span className="fav-art fav-art-sm" aria-hidden>
              <Heart fill="currentColor" strokeWidth={0} />
            </span>
          </span>
          <span className="coll-row-text">
            <span className="coll-row-name">좋아요</span>
            <span className="coll-row-sub">
              {[`${count(likedTracks)}곡`, likedAlbums ? `앨범 ${count(likedAlbums)}장` : null].filter(Boolean).join(' · ')}
            </span>
          </span>
          <ChevronRight size={18} className="coll-row-go" />
        </Link>
        {data?.map((p) => (
          <Link key={p.id} href={`/playlists/${p.id}`} className="coll-row">
            <span className="coll-row-art">
              <PlaylistArt playlist={p} size="sm" />
            </span>
            <span className="coll-row-text">
              <span className="coll-row-name">{p.name}</span>
              <span className="coll-row-sub">
                플레이리스트 · {count(p.count)}곡{p.duration ? ` · ${span(p.duration)}` : ''}
              </span>
            </span>
            <ChevronRight size={18} className="coll-row-go" />
          </Link>
        ))}
      </div>
    </div>
  );
}
