/**
 * 프로세스에 하나뿐인 라이브러리.
 *
 * 개발 서버는 모듈을 여러 번 불러오므로 `globalThis`에 둔다. 그렇지 않으면 화면과
 * API가 서로 다른 라이브러리를 보게 된다.
 */
import fs from 'node:fs/promises';
import path from 'node:path';
import { connection } from 'next/server';
import { config } from '../config';
import { buildLibrary, emptyIndex, type Library } from './build';
import { scanLibrary, type IndexFile } from './scanner';
import type { ScanStatus } from './types';

interface State {
  lib: Library;
  index: IndexFile | null;
  status: ScanStatus;
  loaded: Promise<void> | null;
  scan: Promise<void> | null;
}

const g = globalThis as typeof globalThis & { __sori?: State };

function state(): State {
  return (g.__sori ??= {
    lib: buildLibrary(emptyIndex()),
    index: null,
    status: { state: 'idle', processed: 0, total: 0 },
    loaded: null,
    scan: null,
  });
}

const indexPath = () => path.join(config.dataDir, 'library.json');

async function readIndex(): Promise<IndexFile | null> {
  try {
    return JSON.parse(await fs.readFile(indexPath(), 'utf8')) as IndexFile;
  } catch {
    return null;
  }
}

/** 쓰다 꺼져도 반쪽짜리 파일이 남지 않게 임시 파일에 쓰고 바꿔 끼운다. */
async function writeIndex(index: IndexFile) {
  await fs.mkdir(config.dataDir, { recursive: true });
  const tmp = `${indexPath()}.${process.pid}.tmp`;
  await fs.writeFile(tmp, JSON.stringify(index));
  await fs.rename(tmp, indexPath());
}

/**
 * 라이브러리를 돌려준다. 처음 부르면 디스크의 색인을 읽고, 뒤에서 조용히 다시
 * 스캔해 그 사이 더해지거나 빠진 파일을 맞춘다.
 */
export async function getLibrary(): Promise<Library> {
  const s = state();
  s.loaded ??= (async () => {
    const idx = await readIndex();
    if (idx) {
      s.index = idx;
      s.lib = buildLibrary(idx);
    }
    startScan();
  })();
  await s.loaded;
  return s.lib;
}

/** 페이지에서 쓰는 쪽. 요청 때마다 그리도록 `connection()`을 먼저 부른다. */
export async function loadLibrary(): Promise<Library> {
  await connection();
  return getLibrary();
}

export function getScanStatus(): ScanStatus {
  return state().status;
}

export function startScan(): ScanStatus {
  const s = state();
  if (s.scan) return s.status;
  const startedAt = Date.now();
  s.status = { state: 'scanning', phase: 'listing', processed: 0, total: 0, startedAt };
  s.scan = (async () => {
    try {
      const result = await scanLibrary(s.index, (p) => {
        s.status = { ...s.status, ...p };
      });
      s.status = { ...s.status, phase: 'saving' };
      await writeIndex(result.index);
      s.index = result.index;
      s.lib = buildLibrary(result.index);
      const n = result.index.tracks.length;
      s.status = {
        state: 'idle',
        processed: n,
        total: n,
        startedAt,
        finishedAt: Date.now(),
        added: result.added,
        removed: result.removed,
      };
    } catch (e) {
      s.status = {
        state: 'error',
        processed: 0,
        total: 0,
        startedAt,
        finishedAt: Date.now(),
        error: e instanceof Error ? e.message : String(e),
      };
    } finally {
      s.scan = null;
    }
  })();
  return s.status;
}

/** 스캔이 끝날 때까지 기다린다. 테스트와 CLI에서 쓴다. */
export async function waitForScan() {
  await state().scan;
}
