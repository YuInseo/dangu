/**
 * 색인(곡 목록) → 앨범·아티스트·장르.
 *
 * 저장하는 것은 곡뿐이고 나머지는 전부 여기서 다시 계산한다. 묶는 규칙을 고쳐도
 * 색인을 지우고 다시 스캔할 필요가 없다.
 */
import { UNKNOWN_ARTIST, type IndexFile } from './scanner';
import { artistId, genreId, norm } from './ids';
import { betterFormat, tierOf } from '../quality';
import type { Album, AlbumCard, Artist, Genre, LibrarySummary, StoredTrack, Track } from './types';

export interface Library {
  tracks: Map<string, StoredTrack>;
  views: Map<string, Track>;
  albums: Map<string, Album>;
  artists: Map<string, Artist>;
  genres: Map<string, Genre>;
  /** 앨범 아티스트 → 연도 순. */
  albumList: Album[];
  artistList: Artist[];
  genreList: Genre[];
  /** 아티스트 → 앨범 → 디스크 → 트랙 순. */
  trackList: Track[];
  summary: LibrarySummary;
  /** 검색용으로 미리 정규화해 둔 문자열. */
  haystack: { albums: Map<string, string>; artists: Map<string, string>; tracks: Map<string, string> };
}

export const VARIOUS = 'Various Artists';
export const collator = new Intl.Collator('ko', { numeric: true, sensitivity: 'base' });

/** 정렬할 때는 "The Beatles"를 B에 둔다. */
export function sortName(name: string): string {
  return name.replace(/^the\s+/i, '');
}

/** 아티스트 이름 순. 이름을 모르는 것은 맨 뒤로. */
export function compareArtist(a: string, b: string): number {
  const ua = a === UNKNOWN_ARTIST || a === VARIOUS ? 1 : 0;
  const ub = b === UNKNOWN_ARTIST || b === VARIOUS ? 1 : 0;
  return ua - ub || collator.compare(sortName(a), sortName(b));
}

function mostCommon(xs: string[]): string | undefined {
  const counts = new Map<string, number>();
  let best: string | undefined;
  let bestN = 0;
  for (const x of xs) {
    const n = (counts.get(x) ?? 0) + 1;
    counts.set(x, n);
    if (n > bestN) {
      best = x;
      bestN = n;
    }
  }
  return best;
}

export function emptyIndex(): IndexFile {
  return { version: 0, root: '', scannedAt: 0, tracks: [], covers: {} };
}

export function toCard(a: Album): AlbumCard {
  return {
    id: a.id,
    title: a.title,
    artist: a.artist,
    artistId: a.artistId,
    year: a.year,
    addedAt: a.addedAt,
    cover: a.cover,
    format: a.format,
  };
}

export function buildLibrary(index: IndexFile): Library {
  const tracks = new Map(index.tracks.map((t) => [t.id, t]));

  const groups = new Map<string, StoredTrack[]>();
  for (const t of index.tracks) {
    const g = groups.get(t.albumId) ?? [];
    g.push(t);
    groups.set(t.albumId, g);
  }

  const albums = new Map<string, Album>();
  const artists = new Map<string, Artist>();
  const getArtist = (name: string): Artist => {
    const id = artistId(name);
    let a = artists.get(id);
    if (!a) {
      a = { id, name, albumIds: [], appearsOn: [] };
      artists.set(id, a);
    }
    return a;
  };

  for (const [id, list] of groups) {
    list.sort((a, b) => (a.discNo ?? 1) - (b.discNo ?? 1) || (a.trackNo ?? 1e6) - (b.trackNo ?? 1e6) || collator.compare(a.path, b.path));
    const tag = list.find((t) => t.albumArtistTag)?.albumArtistTag;
    const trackArtists = new Set(list.map((t) => norm(t.artist)));
    const artist = tag || (trackArtists.size === 1 ? list[0]!.artist : VARIOUS);
    const years = list.map((t) => t.year).filter((y): y is number => !!y);
    const genreCounts = new Map<string, number>();
    for (const t of list) for (const g of t.genres) genreCounts.set(g, (genreCounts.get(g) ?? 0) + 1);
    const cover = index.covers[id];

    const album: Album = {
      id,
      title: mostCommon(list.map((t) => t.album)) ?? list[0]!.album,
      artist,
      artistId: artistId(artist),
      year: years.length ? Math.min(...years) : undefined,
      genres: [...genreCounts.entries()]
        .sort((a, b) => b[1] - a[1])
        .slice(0, 3)
        .map(([g]) => g),
      trackIds: list.map((t) => t.id),
      discCount: new Set(list.map((t) => t.discNo ?? 1)).size,
      duration: list.reduce((s, t) => s + t.duration, 0),
      addedAt: Math.max(...list.map((t) => t.addedAt)),
      cover: cover?.v ? { v: cover.v, color: cover.color } : undefined,
      format: list.map((t) => t.format).reduce(betterFormat),
    };
    albums.set(id, album);
    getArtist(artist).albumIds.push(id);
  }

  const views = new Map<string, Track>();
  for (const t of index.tracks) {
    const album = albums.get(t.albumId)!;
    const primary = t.artists[0] ?? t.artist;
    const { path: _p, mtime: _m, size: _s, albumArtistTag: _a, ...rest } = t;
    views.set(t.id, {
      ...rest,
      artistId: artistId(primary),
      albumArtist: album.artist,
      albumArtistId: album.artistId,
      cover: album.cover,
    });
    for (const name of t.artists) {
      if (norm(name) === norm(album.artist)) continue;
      getArtist(name).appearsOn.push(t.id);
    }
    // 곡 아티스트가 "A feat. B"처럼 artists에 없더라도 링크가 끊기지 않게.
    if (!t.artists.some((n) => norm(n) === norm(primary))) getArtist(primary);
  }

  const albumList = [...albums.values()].sort(
    (a, b) => compareArtist(a.artist, b.artist) || (a.year ?? 0) - (b.year ?? 0) || collator.compare(a.title, b.title),
  );
  const order = new Map(albumList.map((a, i) => [a.id, i]));

  for (const a of artists.values()) {
    a.albumIds.sort((x, y) => (albums.get(y)!.year ?? 0) - (albums.get(x)!.year ?? 0));
    const coverAlbum =
      a.albumIds.map((id) => albums.get(id)!).find((al) => al.cover) ??
      a.appearsOn.map((id) => albums.get(tracks.get(id)!.albumId)!).find((al) => al.cover);
    a.coverAlbumId = coverAlbum?.id;
    a.cover = coverAlbum?.cover;
  }

  const artistList = [...artists.values()]
    .filter((a) => norm(a.name) !== norm(VARIOUS) && (a.albumIds.length || a.appearsOn.length))
    .sort((a, b) => compareArtist(a.name, b.name));

  const genres = new Map<string, Genre>();
  for (const al of albumList) {
    for (const name of al.genres) {
      const id = genreId(name);
      let g = genres.get(id);
      if (!g) {
        g = { id, name, albumIds: [], covers: [] };
        genres.set(id, g);
      }
      g.albumIds.push(al.id);
      if (al.cover && g.covers.length < 4) g.covers.push({ albumId: al.id, cover: al.cover });
    }
  }
  const genreList = [...genres.values()].sort((a, b) => b.albumIds.length - a.albumIds.length || collator.compare(a.name, b.name));

  const trackList = [...views.values()].sort(
    (a, b) => order.get(a.albumId)! - order.get(b.albumId)! || (a.discNo ?? 1) - (b.discNo ?? 1) || (a.trackNo ?? 1e6) - (b.trackNo ?? 1e6),
  );

  let hires = 0;
  let lossless = 0;
  for (const al of albums.values()) {
    const tier = tierOf(al.format);
    if (tier === 'hires') hires++;
    if (tier !== 'lossy') lossless++;
  }

  const summary: LibrarySummary = {
    albums: albums.size,
    artists: artistList.length,
    tracks: tracks.size,
    duration: index.tracks.reduce((s, t) => s + t.duration, 0),
    bytes: index.tracks.reduce((s, t) => s + t.size, 0),
    hires,
    lossless,
    scannedAt: index.scannedAt || undefined,
  };

  const haystack = {
    albums: new Map(albumList.map((a) => [a.id, norm(`${a.title} ${a.artist}`)])),
    artists: new Map(artistList.map((a) => [a.id, norm(a.name)])),
    tracks: new Map(trackList.map((t) => [t.id, norm(`${t.title} ${t.artist} ${t.album}`)])),
  };

  return { tracks, views, albums, artists, genres, albumList, artistList, genreList, trackList, summary, haystack };
}

export interface SearchResult {
  artists: Artist[];
  albums: AlbumCard[];
  tracks: Track[];
}

/** 띄어쓴 낱말이 모두 들어 있으면 맞은 것. 앞에서부터 맞으면 위로 올린다. */
export function searchLibrary(lib: Library, q: string): SearchResult {
  const query = norm(q);
  if (!query) return { artists: [], albums: [], tracks: [] };
  const terms = query.split(' ');
  const rank = <T extends { id: string }>(items: T[], hay: Map<string, string>, limit: number) => {
    const hits: { item: T; score: number }[] = [];
    for (const item of items) {
      const h = hay.get(item.id);
      if (!h || !terms.every((t) => h.includes(t))) continue;
      hits.push({ item, score: h.startsWith(query) ? 0 : h.includes(` ${terms[0]}`) ? 1 : 2 });
      if (hits.length > limit * 4) break;
    }
    return hits
      .sort((a, b) => a.score - b.score)
      .slice(0, limit)
      .map((h) => h.item);
  };
  return {
    artists: rank(lib.artistList, lib.haystack.artists, 12),
    albums: rank(lib.albumList, lib.haystack.albums, 24).map(toCard),
    tracks: rank(lib.trackList, lib.haystack.tracks, 50),
  };
}
