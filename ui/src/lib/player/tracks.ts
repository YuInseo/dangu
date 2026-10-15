/**
 * 라이브러리 곡과 라이브 아카이브 곡을 `PlayableTrack`으로.
 */
import type { CoverRef, Track } from '../library/types';
import { prettyDate, type LiveShow, type LiveTrack } from '../live/shared';
import type { PlayableTrack } from './types';

export function coverUrls(albumId: string, cover?: CoverRef) {
  if (!cover) return undefined;
  return {
    sm: `/api/cover/${albumId}?s=sm&v=${cover.v}`,
    lg: `/api/cover/${albumId}?s=lg&v=${cover.v}`,
    color: cover.color,
  };
}

export function fromLibrary(t: Track): PlayableTrack {
  return {
    key: `lib:${t.id}`,
    source: 'library',
    title: t.title,
    artist: t.artist,
    album: t.album,
    albumHref: `/albums/${t.albumId}`,
    artistHref: `/artists/${t.artistId}`,
    albumId: t.albumId,
    year: t.year,
    trackNo: t.trackNo,
    discNo: t.discNo,
    duration: t.duration,
    format: t.format,
    src: `/api/stream/${t.id}`,
    transcodeSrc: `/api/stream/${t.id}?transcode=1`,
    dspSrc: `/api/stream/${t.id}?`,
    loudness: t.loudness,
    cover: coverUrls(t.albumId, t.cover),
    art: { seed: t.albumId, title: t.album, subtitle: t.albumArtist },
  };
}

export function fromLive(show: Pick<LiveShow, 'id' | 'band' | 'title' | 'date' | 'venue'>, t: LiveTrack): PlayableTrack {
  const when = prettyDate(show.date);
  return {
    key: `live:${show.id}/${t.file}`,
    source: 'live',
    title: t.title,
    artist: show.band,
    album: [when, show.venue].filter(Boolean).join(' · ') || show.title,
    albumHref: `/live/${encodeURIComponent(show.id)}`,
    artistHref: `/live?q=${encodeURIComponent(show.band)}`,
    trackNo: t.trackNo,
    year: show.date ? Number(show.date.slice(0, 4)) : undefined,
    duration: t.duration,
    format: t.format,
    src: t.url,
    dspSrc: `/api/live/stream?id=${encodeURIComponent(show.id)}&file=${encodeURIComponent(t.file)}&`,
    art: { seed: show.id, title: show.band, subtitle: when || undefined },
    probe: t.format.codec === 'FLAC' ? { id: show.id, file: t.file } : undefined,
    live: { id: show.id, file: t.file },
  };
}
