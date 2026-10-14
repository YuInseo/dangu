/**
 * Internet Archive의 Live Music Archive(etree 컬렉션).
 *
 * 밴드가 녹음과 무료 공유를 허락한 공연 실황 모음이다. 원본 대부분이 FLAC이라
 * 무료이면서 무손실인, 합법적인 음원이다. 키도 가입도 필요 없다.
 */
import type { AudioFormat } from '../library/types';
import type { LiveShow, LiveShowDetail, LiveSort, LiveTrack } from './shared';

export type { LiveShow, LiveShowDetail, LiveSort, LiveTrack } from './shared';

const IA = 'https://archive.org';

const SORTS: Record<LiveSort, string> = {
  popular: 'downloads desc',
  recent: 'publicdate desc',
  newest: 'date desc',
};

// ---------------------------------------------------------------------------
// 작은 캐시. 같은 공연을 여러 번 열 때 archive.org를 다시 부르지 않는다.

const g = globalThis as typeof globalThis & { __soriLive?: Map<string, { at: number; value: unknown }> };
const cache = (g.__soriLive ??= new Map());

async function cached<T>(key: string, ttlMs: number, load: () => Promise<T>): Promise<T> {
  const hit = cache.get(key);
  if (hit && Date.now() - hit.at < ttlMs) return hit.value as T;
  const value = await load();
  cache.set(key, { at: Date.now(), value });
  if (cache.size > 500) cache.delete(cache.keys().next().value!);
  return value;
}

async function getJson(url: string): Promise<unknown> {
  const res = await fetch(url, {
    cache: 'no-store',
    headers: { 'User-Agent': 'Sori/0.1 (self-hosted music player)' },
    signal: AbortSignal.timeout(12_000),
  });
  if (!res.ok) throw new Error(`archive.org ${res.status}`);
  return res.json();
}

// ---------------------------------------------------------------------------
// archive.org의 필드는 문자열일 때도, 배열일 때도 있다.

type Field = string | string[] | number | undefined | null;

function one(v: Field): string | undefined {
  if (Array.isArray(v)) return v[0]?.toString();
  return v == null ? undefined : String(v);
}

function many(v: Field): string[] {
  if (Array.isArray(v)) return v.map(String);
  return v == null ? [] : [String(v)];
}

export function qualityFromFormats(formats: string[]): LiveShow['quality'] {
  const f = formats.map((x) => x.toLowerCase());
  if (f.some((x) => x.includes('24bit flac'))) return 'flac24';
  if (f.some((x) => x === 'flac')) return 'flac';
  if (f.some((x) => x.includes('mp3') || x.includes('ogg'))) return 'lossy';
  return 'unknown';
}

function showDate(v: Field): string | undefined {
  const s = one(v);
  return s ? s.slice(0, 10) : undefined;
}

/** Lucene 구문으로 해석될 글자를 지워 구절 검색으로만 쓴다. */
function phrase(q: string): string {
  return q.replace(/["\\]/g, ' ').trim();
}

export function buildSearchUrl(opts: { q?: string; sort?: LiveSort; page?: number; rows?: number }): string {
  const parts = ['collection:etree', 'mediatype:etree'];
  const q = opts.q ? phrase(opts.q) : '';
  if (q) parts.push(`(creator:("${q}") OR title:("${q}") OR venue:("${q}"))`);
  const params = new URLSearchParams();
  params.set('q', parts.join(' AND '));
  for (const f of ['identifier', 'title', 'creator', 'date', 'venue', 'coverage', 'downloads', 'format']) params.append('fl[]', f);
  params.append('sort[]', SORTS[opts.sort ?? 'popular']);
  params.set('rows', String(opts.rows ?? 48));
  params.set('page', String(opts.page ?? 1));
  params.set('output', 'json');
  return `${IA}/advancedsearch.php?${params}`;
}

interface SearchDoc {
  identifier: string;
  title?: Field;
  creator?: Field;
  date?: Field;
  venue?: Field;
  coverage?: Field;
  downloads?: number;
  format?: Field;
}

export function parseSearch(json: unknown): { shows: LiveShow[]; total: number } {
  const r = (json as { response?: { numFound?: number; docs?: SearchDoc[] } }).response;
  const shows = (r?.docs ?? []).map(
    (d): LiveShow => ({
      id: d.identifier,
      title: one(d.title) ?? d.identifier,
      band: one(d.creator) ?? '알 수 없는 밴드',
      date: showDate(d.date),
      venue: one(d.venue),
      city: one(d.coverage),
      downloads: typeof d.downloads === 'number' ? d.downloads : undefined,
      quality: qualityFromFormats(many(d.format)),
    }),
  );
  return { shows, total: r?.numFound ?? shows.length };
}

export async function searchShows(opts: { q?: string; sort?: LiveSort; page?: number; rows?: number }) {
  const url = buildSearchUrl(opts);
  return cached(`s:${url}`, 10 * 60_000, async () => parseSearch(await getJson(url)));
}

// ---------------------------------------------------------------------------
// 공연 하나

interface IaFile {
  name: string;
  format?: string;
  source?: string;
  title?: string;
  track?: string;
  length?: string;
  bitrate?: string;
  private?: string | boolean;
}

/** "343.12", "05:43", "1:05:43" 모두 초로. */
export function parseLength(s?: string): number {
  if (!s) return 0;
  if (s.includes(':')) return s.split(':').reduce((acc, p) => acc * 60 + (Number(p) || 0), 0);
  return Number(s) || 0;
}

/** 원하는 순서. 앞의 것이 하나라도 있으면 그 형식만 쓴다. */
const PREFERENCE: { match: (f: string) => boolean; format: (file: IaFile) => AudioFormat }[] = [
  {
    match: (f) => f === '24bit flac',
    format: () => ({ codec: 'FLAC', container: 'flac', lossless: true, bitDepth: 24, mime: 'audio/flac' }),
  },
  {
    match: (f) => f === 'flac',
    format: () => ({ codec: 'FLAC', container: 'flac', lossless: true, mime: 'audio/flac' }),
  },
  {
    match: (f) => f === 'vbr mp3' || f === '320kbps mp3',
    format: (file) => ({
      codec: 'MP3',
      container: 'mp3',
      lossless: false,
      bitrate: file.bitrate ? Number(file.bitrate) * 1000 || undefined : undefined,
      mime: 'audio/mpeg',
    }),
  },
  {
    match: (f) => f === 'ogg vorbis',
    format: () => ({ codec: 'Vorbis', container: 'ogg', lossless: false, mime: 'audio/ogg; codecs="vorbis"' }),
  },
];

function downloadUrl(id: string, name: string) {
  return `${IA}/download/${encodeURIComponent(id)}/${name.split('/').map(encodeURIComponent).join('/')}`;
}

/** 공연 설명은 HTML이 섞인 글이다. 태그는 걷어내고 줄바꿈만 살린다. */
export function htmlToText(html?: string): string | undefined {
  if (!html) return undefined;
  const text = html
    .replace(/<br\s*\/?>/gi, '\n')
    .replace(/<\/(p|div|li)>/gi, '\n')
    .replace(/<[^>]+>/g, '')
    .replace(/&nbsp;/g, ' ')
    .replace(/&amp;/g, '&')
    .replace(/&lt;/g, '<')
    .replace(/&gt;/g, '>')
    .replace(/&quot;/g, '"')
    .replace(/&#39;/g, "'")
    .replace(/\r/g, '')
    .replace(/\n{3,}/g, '\n\n')
    .trim();
  return text || undefined;
}

export function parseShow(id: string, json: unknown): LiveShowDetail | null {
  const j = json as { metadata?: Record<string, Field>; files?: IaFile[]; is_dark?: boolean };
  const m = j.metadata;
  if (!m || j.is_dark) return null;
  const files = (j.files ?? []).filter((f) => f.private !== 'true' && f.private !== true && f.format);

  let tracks: LiveTrack[] = [];
  for (const pref of PREFERENCE) {
    const chosen = files.filter((f) => pref.match(f.format!.toLowerCase()));
    if (!chosen.length) continue;
    tracks = chosen.map((f) => {
      const base = f.name
        .split('/')
        .pop()!
        .replace(/\.[^.]+$/, '');
      const no = f.track ? parseInt(f.track, 10) : NaN;
      return {
        file: f.name,
        title: f.title?.trim() || base,
        trackNo: Number.isFinite(no) ? no : undefined,
        duration: parseLength(f.length),
        format: pref.format(f),
        url: downloadUrl(id, f.name),
      };
    });
    break;
  }
  const collator = new Intl.Collator('en', { numeric: true });
  tracks.sort((a, b) =>
    a.trackNo != null && b.trackNo != null && a.trackNo !== b.trackNo ? a.trackNo - b.trackNo : collator.compare(a.file, b.file),
  );

  return {
    id,
    title: one(m.title) ?? id,
    band: one(m.creator) ?? '알 수 없는 밴드',
    date: showDate(m.date),
    venue: one(m.venue),
    city: one(m.coverage),
    downloads: undefined,
    quality: qualityFromFormats(files.map((f) => f.format!)),
    notes: htmlToText(one(m.description)),
    source: one(m.source),
    lineage: one(m.lineage),
    taper: one(m.taper),
    tracks,
  };
}

const ID = /^[A-Za-z0-9._-]{1,120}$/;

export function isValidId(id: string) {
  return ID.test(id);
}

export async function getShow(id: string): Promise<LiveShowDetail | null> {
  if (!isValidId(id)) return null;
  return cached(`m:${id}`, 30 * 60_000, async () => parseShow(id, await getJson(`${IA}/metadata/${encodeURIComponent(id)}`)));
}

/** 파일 앞부분만 받아 STREAMINFO를 읽는다. */
export async function probeFlac(id: string, file: string) {
  if (!isValidId(id) || file.includes('..') || file.startsWith('/') || !/\.flac$/i.test(file)) return null;
  const { parseFlacStreamInfo } = await import('./flac');
  return cached(`p:${id}/${file}`, 24 * 60 * 60_000, async () => {
    const res = await fetch(downloadUrl(id, file), {
      headers: { Range: 'bytes=0-65535', 'User-Agent': 'Sori/0.1 (self-hosted music player)' },
      signal: AbortSignal.timeout(12_000),
    });
    if (!res.ok && res.status !== 206) return null;
    const reader = res.body?.getReader();
    if (!reader) return null;
    // 범위 요청을 무시하고 전체를 보내는 서버도 있으니 64KB에서 끊는다.
    const chunks: Uint8Array[] = [];
    let total = 0;
    while (total < 65536) {
      const { done, value } = await reader.read();
      if (done) break;
      chunks.push(value);
      total += value.length;
    }
    await reader.cancel().catch(() => {});
    const buf = new Uint8Array(total);
    let o = 0;
    for (const c of chunks) {
      buf.set(c, o);
      o += c.length;
    }
    return parseFlacStreamInfo(buf);
  });
}
