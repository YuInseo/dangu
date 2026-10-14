/**
 * 앨범 표지.
 *
 * 폴더의 cover.jpg 같은 파일을 먼저 보고, 없으면 첫 곡에 박힌 그림을 꺼낸다. 원본이
 * 3000px짜리 PNG여도 그리드에는 480px, 전체 화면에는 1400px WebP를 보낸다 — 앨범
 * 수백 장이 한 화면에 뜨는 곳이라 원본을 그대로 보내면 스크롤이 멎는다.
 */
import fs from 'node:fs/promises';
import path from 'node:path';
import sharp from 'sharp';
import { parseFile, selectCover } from 'music-metadata';
import { config } from '../config';
import { pool } from '../pool';
import { makeId } from './ids';
import type { StoredTrack } from './types';

export interface CoverEntry {
  /** 어디서 왔는지와 그 파일의 수정 시각. 같으면 다시 만들지 않는다. */
  sig: string;
  /** 비어 있으면 표지가 없다. */
  v: string;
  color: string;
}

export type CoverSize = 'sm' | 'lg';
export const COVER_PX: Record<CoverSize, number> = { sm: 480, lg: 1400 };

export function coversDir() {
  return path.join(config.dataDir, 'covers');
}

export function coverFile(albumId: string, v: string, size: CoverSize) {
  return path.join(coversDir(), `${albumId}-${v}-${size}.webp`);
}

const IMAGE = /\.(jpe?g|png|webp|gif|bmp|tiff?)$/i;
const PREFERRED = ['cover', 'folder', 'front', 'album', 'artwork', 'albumart', 'art'];

interface Source {
  sig: string;
  load: () => Promise<Buffer | null>;
}

async function findSource(root: string, tracks: StoredTrack[], dirCache: Map<string, string[]>): Promise<Source | null> {
  const dirs = new Set<string>();
  for (const t of tracks) {
    const d = path.posix.dirname(t.path);
    dirs.add(d);
    // CD1/CD2 폴더의 앨범은 표지가 부모 폴더에 있는 경우가 많다.
    if (/^(cd|disc|disk|디스크)\s*[-_.]?\s*\d+/i.test(path.posix.basename(d))) dirs.add(path.posix.dirname(d));
  }

  for (const d of dirs) {
    let images = dirCache.get(d);
    if (!images) {
      try {
        const entries = await fs.readdir(path.join(root, d), { withFileTypes: true });
        images = entries.filter((e) => e.isFile() && IMAGE.test(e.name) && !e.name.startsWith('.')).map((e) => e.name);
      } catch {
        images = [];
      }
      dirCache.set(d, images);
    }
    const pick =
      PREFERRED.map((p) => images.find((n) => n.toLowerCase().replace(IMAGE, '') === p)).find(Boolean) ??
      PREFERRED.map((p) => images.find((n) => n.toLowerCase().startsWith(p))).find(Boolean) ??
      (images.length === 1 ? images[0] : undefined);
    if (pick) {
      const abs = path.join(root, d, pick);
      const st = await fs.stat(abs).catch(() => null);
      if (st) return { sig: `file:${d}/${pick}:${Math.floor(st.mtimeMs)}:${st.size}`, load: () => fs.readFile(abs) };
    }
  }

  // 폴더에 그림이 없으면 곡에 박힌 그림. 앞의 두 곡까지만 본다.
  const first = tracks.slice(0, 2);
  if (!first.length) return null;
  return {
    sig: `emb:${first.map((t) => `${t.path}:${t.mtime}`).join('|')}`,
    load: async () => {
      for (const t of first) {
        try {
          const meta = await parseFile(path.join(root, t.path), { skipCovers: false, duration: false });
          const pic = selectCover(meta.common.picture);
          if (pic?.data?.length) return Buffer.from(pic.data);
        } catch {
          // 다음 곡
        }
      }
      return null;
    },
  };
}

function hex(c: { r: number; g: number; b: number }) {
  return `#${[c.r, c.g, c.b].map((x) => Math.round(x).toString(16).padStart(2, '0')).join('')}`;
}

async function exists(p: string) {
  return fs.access(p).then(
    () => true,
    () => false,
  );
}

export async function updateCovers(
  root: string,
  tracks: StoredTrack[],
  prev: Record<string, CoverEntry>,
  report: (processed: number, total: number) => void,
): Promise<Record<string, CoverEntry>> {
  await fs.mkdir(coversDir(), { recursive: true });

  const byAlbum = new Map<string, StoredTrack[]>();
  for (const t of tracks) {
    const list = byAlbum.get(t.albumId) ?? [];
    list.push(t);
    byAlbum.set(t.albumId, list);
  }
  for (const list of byAlbum.values()) {
    list.sort((a, b) => (a.discNo ?? 1) - (b.discNo ?? 1) || (a.trackNo ?? 0) - (b.trackNo ?? 0) || a.path.localeCompare(b.path));
  }

  const result: Record<string, CoverEntry> = {};
  const dirCache = new Map<string, string[]>();
  const albums = [...byAlbum.entries()];
  let done = 0;
  report(0, albums.length);

  await pool(albums, 4, async ([albumId, list]) => {
    try {
      const src = await findSource(root, list, dirCache);
      if (!src) return;
      const before = prev[albumId];
      if (before && before.sig === src.sig && (!before.v || (await exists(coverFile(albumId, before.v, 'sm'))))) {
        result[albumId] = before;
        return;
      }
      const buf = await src.load();
      if (!buf) {
        result[albumId] = { sig: src.sig, v: '', color: '' };
        return;
      }
      const v = makeId(src.sig).slice(0, 8);
      const img = sharp(buf, { failOn: 'none' }).rotate();
      await img
        .clone()
        .resize(COVER_PX.sm, COVER_PX.sm, { fit: 'cover' })
        .webp({ quality: 82 })
        .toFile(coverFile(albumId, v, 'sm'));
      await img
        .clone()
        .resize(COVER_PX.lg, COVER_PX.lg, { fit: 'cover', withoutEnlargement: true })
        .webp({ quality: 88 })
        .toFile(coverFile(albumId, v, 'lg'));
      const { dominant } = await img.clone().resize(64, 64, { fit: 'cover' }).stats();
      result[albumId] = { sig: src.sig, v, color: hex(dominant) };
    } catch {
      // 깨진 그림이면 표지 없는 앨범으로 둔다.
    } finally {
      done++;
      if (done % 10 === 0) report(done, albums.length);
    }
  });
  report(albums.length, albums.length);

  // 이제 아무도 가리키지 않는 표지 파일을 지운다.
  const keep = new Set<string>();
  for (const [id, e] of Object.entries(result)) {
    if (!e.v) continue;
    keep.add(path.basename(coverFile(id, e.v, 'sm')));
    keep.add(path.basename(coverFile(id, e.v, 'lg')));
  }
  for (const name of await fs.readdir(coversDir())) {
    if (!keep.has(name)) await fs.rm(path.join(coversDir(), name), { force: true });
  }
  return result;
}
