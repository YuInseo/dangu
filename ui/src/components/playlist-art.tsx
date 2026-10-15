'use client';

import type { PlaylistSummary } from '@/lib/collections';
import { coverUrls } from '@/lib/player/tracks';
import { Cover } from './cover';

/**
 * 플레이리스트 표지. 서로 다른 앨범이 넷 이상이면 넷을 2×2로, 아니면 첫 앨범의 표지를,
 * 비었으면 이름으로 그린 그림을.
 */
export function PlaylistArt({ playlist, size = 'md' }: { playlist: Pick<PlaylistSummary, 'id' | 'name' | 'covers'>; size?: 'sm' | 'md' | 'lg' }) {
  const covers = playlist.covers;
  if (covers.length >= 4) {
    return (
      <div className={`pl-mosaic pl-mosaic-${size}`} aria-hidden>
        {covers.slice(0, 4).map((c) => {
          const u = coverUrls(c.albumId, c.cover);
          return <Cover key={c.albumId} src={u?.sm} color={u?.color} alt="" seed={c.albumId} title={c.title} size="sm" />;
        })}
      </div>
    );
  }
  const first = covers[0];
  const u = first ? coverUrls(first.albumId, first.cover) : undefined;
  return (
    <Cover
      src={size === 'lg' ? u?.lg : u?.sm}
      color={u?.color}
      alt=""
      seed={first?.albumId ?? playlist.id}
      title={first ? first.title : playlist.name}
      subtitle={first ? first.artist : '플레이리스트'}
      size={size}
    />
  );
}
