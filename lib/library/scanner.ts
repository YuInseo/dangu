/**
 * 음악 폴더를 읽어 색인을 만든다.
 *
 * 두 번째 스캔부터는 파일의 수정 시각과 크기만 보고, 바뀐 파일만 태그를 다시 읽는다.
 * 수만 곡짜리 라이브러리도 재시작할 때마다 몇 초면 끝나는 이유다.
 */
import fs from 'node:fs/promises';
import path from 'node:path';
import { parseFile } from 'music-metadata';
import { config } from '../config';
import { pool } from '../pool';
import { updateCovers, type CoverEntry } from './covers';
import { AUDIO_EXTENSIONS, toAudioFormat } from './formats';
import { makeId, norm } from './ids';
import type { ScanStatus, StoredTrack } from './types';

export const INDEX_VERSION = 4;

export interface IndexFile {
  version: number;
  root: string;
  scannedAt: number;
  tracks: StoredTrack[];
  /** 앨범 ID → 표지. `v`가 비어 있으면 "찾아봤는데 없었다". */
  covers: Record<string, CoverEntry>;
}

export interface ScanResult {
  index: IndexFile;
  added: number;
  removed: number;
  failed: number;
}

interface FileEntry {
  abs: string;
  rel: string;
  mtime: number;
  size: number;
  born: number;
}

/** 숨김 파일(맥의 `._xxx.flac` 포함)은 건너뛴다. 심볼릭 링크는 따라가되 고리는 끊는다. */
async function listAudioFiles(root: string): Promise<FileEntry[]> {
  const out: FileEntry[] = [];
  const seen = new Set<string>();

  async function walk(dir: string): Promise<void> {
    let real: string;
    try {
      real = await fs.realpath(dir);
    } catch {
      return;
    }
    if (seen.has(real)) return;
    seen.add(real);

    let entries;
    try {
      entries = await fs.readdir(dir, { withFileTypes: true });
    } catch {
      return;
    }
    for (const e of entries) {
      if (e.name.startsWith('.') || e.name.startsWith('@')) continue; // .DS_Store, ._*, 시놀로지 @eaDir
      const abs = path.join(dir, e.name);
      let isDir = e.isDirectory();
      let isFile = e.isFile();
      if (e.isSymbolicLink()) {
        try {
          const st = await fs.stat(abs);
          isDir = st.isDirectory();
          isFile = st.isFile();
        } catch {
          continue;
        }
      }
      if (isDir) {
        await walk(abs);
        continue;
      }
      if (!isFile) continue;
      const ext = path.extname(e.name).slice(1).toLowerCase();
      if (!AUDIO_EXTENSIONS.has(ext)) continue;
      try {
        const st = await fs.stat(abs);
        out.push({
          abs,
          rel: path.relative(root, abs).split(path.sep).join('/'),
          mtime: Math.floor(st.mtimeMs),
          size: st.size,
          born: Math.floor(st.birthtimeMs || st.mtimeMs),
        });
      } catch {
        // 읽는 사이에 사라진 파일
      }
    }
  }

  await walk(root);
  return out;
}

export const UNKNOWN_ARTIST = '알 수 없는 아티스트';
export const UNKNOWN_ALBUM = '알 수 없는 앨범';

const DISC_DIR = /^(cd|disc|disk|디스크)\s*[-_.]?\s*(\d+)\b/i;

/** "앨범/CD2" 같은 디스크 폴더는 앨범 폴더 하나로 친다. */
export function albumDir(relDir: string): string {
  if (relDir === '.' || relDir === '') return '';
  const base = path.posix.basename(relDir);
  return DISC_DIR.test(base) ? path.posix.dirname(relDir).replace(/^\.$/, '') : relDir;
}

function discFromDir(relDir: string): number | undefined {
  const m = DISC_DIR.exec(path.posix.basename(relDir));
  return m ? Number(m[2]) : undefined;
}

function clean(s: string | undefined | null): string {
  return (s ?? '').replace(/\u0000/g, '').trim();
}

function uniq(xs: string[]): string[] {
  const seen = new Set<string>();
  return xs.filter((x) => {
    const k = norm(x);
    if (seen.has(k)) return false;
    seen.add(k);
    return true;
  });
}

function parseYear(s?: string): number | undefined {
  const m = /\b(1[89]\d\d|20\d\d)\b/.exec(s ?? '');
  return m ? Number(m[1]) : undefined;
}

/** 태그가 없을 때 "01 - Song_Name" → 번호 1, 제목 "Song Name". */
export function fromFileName(base: string): { no?: number; title: string } {
  const m = /^(\d{1,3})(?:\s*[-._)]\s*|\s+)(.+)$/.exec(base);
  const raw = m ? m[2]! : base;
  return { no: m ? Number(m[1]) : undefined, title: raw.replace(/_/g, ' ').trim() || base };
}

async function readTrack(f: FileEntry, addedAt: number): Promise<StoredTrack | null> {
  const ext = path.extname(f.rel).slice(1).toLowerCase();
  let meta;
  try {
    meta = await parseFile(f.abs, { skipCovers: true, duration: false });
    // 헤더에 길이가 없는 MP3는 프레임을 끝까지 세야 안다. 느리지만 그런 파일에만 한다.
    if (!meta.format.duration) meta = await parseFile(f.abs, { skipCovers: true, duration: true });
  } catch {
    return null;
  }

  const c = meta.common;
  const relDir = path.posix.dirname(f.rel);
  const fileName = fromFileName(path.posix.basename(f.rel).replace(/\.[^.]+$/, ''));
  const dir = albumDir(relDir);

  const artists = uniq((c.artists?.length ? c.artists : c.artist ? [c.artist] : []).map(clean).filter(Boolean));
  const artist = clean(c.artist) || artists.join(', ') || UNKNOWN_ARTIST;
  const album = clean(c.album) || (dir ? path.posix.basename(dir) : '') || UNKNOWN_ALBUM;
  const albumArtistTag = clean(c.albumartist) || undefined;
  const genres = uniq(
    (c.genre ?? [])
      .flatMap((g) => g.split(/\s*;\s*/))
      .map(clean)
      .filter(Boolean),
  );

  // 앨범 아티스트가 적혀 있으면 그걸로, 없으면 폴더로 묶는다. 폴더로 묶어야 태그 없는
  // 컴필레이션이 곡마다 다른 아티스트 때문에 열 조각으로 흩어지지 않는다.
  const key = albumArtistTag ? `aa:${norm(albumArtistTag)}|${norm(album)}` : `dir:${dir}|${norm(album)}`;

  const rg = {
    tg: c.replaygain_track_gain?.dB,
    tp: c.replaygain_track_peak?.ratio,
    ag: c.replaygain_album_gain?.dB,
    ap: c.replaygain_album_peak?.ratio,
  };
  const hasRg = Object.values(rg).some((v) => typeof v === 'number' && Number.isFinite(v));

  return {
    id: makeId(`track:${f.rel}`),
    path: f.rel,
    mtime: f.mtime,
    size: f.size,
    title: clean(c.title) || fileName.title,
    artist,
    artists: artists.length ? artists : [artist],
    album,
    albumId: makeId(`album:${key}`),
    albumArtistTag,
    trackNo: c.track?.no ?? fileName.no,
    discNo: c.disk?.no ?? discFromDir(relDir),
    year: c.year || parseYear(c.date) || parseYear(c.originaldate),
    genres,
    duration: meta.format.duration ?? 0,
    format: toAudioFormat(meta.format, ext),
    addedAt,
    rg: hasRg ? rg : undefined,
  };
}

export async function scanLibrary(prev: IndexFile | null, report: (s: Partial<ScanStatus>) => void): Promise<ScanResult> {
  const root = config.musicDir;
  const st = await fs.stat(root).catch(() => null);
  if (!st?.isDirectory()) throw new Error(`음악 폴더를 찾을 수 없습니다: ${root}`);

  // 폴더를 바꿨거나 색인 형식이 바뀌었으면 처음부터.
  const usable = prev && prev.root === root && prev.version === INDEX_VERSION ? prev : null;
  const old = new Map((usable?.tracks ?? []).map((t) => [t.path, t]));

  report({ phase: 'listing', processed: 0, total: 0 });
  const files = await listAudioFiles(root);
  report({ phase: 'reading', processed: 0, total: files.length });

  const tracks: StoredTrack[] = [];
  let processed = 0;
  let added = 0;
  let failed = 0;
  await pool(files, 8, async (f) => {
    const o = old.get(f.rel);
    if (o && o.mtime === f.mtime && o.size === f.size) {
      tracks.push(o);
    } else {
      const t = await readTrack(f, o?.addedAt ?? f.born);
      if (t) tracks.push(t);
      else failed++;
      if (!o && t) added++;
    }
    processed++;
    if (processed % 20 === 0) report({ processed });
  });
  report({ processed });

  const present = new Set(files.map((f) => f.rel));
  const removed = [...old.keys()].filter((p) => !present.has(p)).length;
  tracks.sort((a, b) => (a.path < b.path ? -1 : a.path > b.path ? 1 : 0));

  const covers = await updateCovers(root, tracks, usable?.covers ?? {}, (processed, total) =>
    report({ phase: 'covers', processed, total }),
  );

  return {
    index: { version: INDEX_VERSION, root, scannedAt: Date.now(), tracks, covers },
    added,
    removed,
    failed,
  };
}
