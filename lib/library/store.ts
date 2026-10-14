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
import { hasFfmpeg } from '../config';
import { readAnalysis, runAnalysis, type AnalysisFile, type AnalysisRun, pendingTracks } from './analysis';
import { buildLibrary, emptyIndex, type Library } from './build';
import { scanLibrary, type IndexFile } from './scanner';
import { getSettings, updateSettings, type ServerSettings } from './settings';
import type { AnalysisStatus, ScanStatus } from './types';

interface State {
  lib: Library;
  index: IndexFile | null;
  status: ScanStatus;
  loaded: Promise<void> | null;
  scan: Promise<void> | null;
  analysis: AnalysisFile;
  run: AnalysisRun | null;
  analysisStatus: AnalysisStatus;
}

const g = globalThis as typeof globalThis & { __sori?: State };

function state(): State {
  return (g.__sori ??= {
    lib: buildLibrary(emptyIndex()),
    index: null,
    status: { state: 'idle', processed: 0, total: 0 },
    loaded: null,
    scan: null,
    analysis: { v: 1, tracks: {} },
    run: null,
    analysisStatus: { speed: 'throttled', state: 'idle', done: 0, total: 0 },
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
    const [idx, analysis] = await Promise.all([readIndex(), readAnalysis()]);
    s.analysis = analysis;
    if (idx) {
      s.index = idx;
      s.lib = buildLibrary(idx, analysis);
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
      s.lib = buildLibrary(result.index, s.analysis);
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
      void startAnalysis();
    }
  })();
  return s.status;
}

// ---------------------------------------------------------------- 오디오 분석

/** 분석을 (다시) 시작한다. 이미 돌고 있으면 멈추고 새로 — 속도를 바꿨을 때. */
export async function startAnalysis() {
  const s = state();
  const settings = await getSettings();
  s.run?.stop();
  s.run = null;
  const tracks = s.index?.tracks ?? [];
  const pending = pendingTracks(tracks, s.analysis).length;
  const analyzed = tracks.length - pending;
  if (settings.analysisSpeed === 'off' || !(await hasFfmpeg()) || !s.index) {
    s.analysisStatus = { speed: settings.analysisSpeed, state: 'off', done: analyzed, total: tracks.length };
    return;
  }
  if (!pending) {
    s.analysisStatus = { speed: settings.analysisSpeed, state: 'idle', done: tracks.length, total: tracks.length };
    return;
  }
  s.analysisStatus = { speed: settings.analysisSpeed, state: 'running', done: analyzed, total: tracks.length };
  const index = s.index;
  const run = runAnalysis({
    root: index.root,
    tracks: index.tracks,
    analysis: s.analysis,
    speed: settings.analysisSpeed,
    onProgress: (n) => {
      if (s.run === run) s.analysisStatus = { ...s.analysisStatus, done: analyzed + n };
    },
    onBatch: () => {
      // 레벨링 값과 파형을 바로 쓰도록 라이브러리를 다시 엮는다
      if (s.index) s.lib = buildLibrary(s.index, s.analysis);
    },
  });
  s.run = run;
  run.done.then(() => {
    if (s.run !== run) return;
    s.run = null;
    const left = pendingTracks(index.tracks, s.analysis).length;
    s.analysisStatus = { ...s.analysisStatus, state: 'idle', done: index.tracks.length - left };
  });
}

export function getAnalysisStatus(): AnalysisStatus {
  return state().analysisStatus;
}

export function getWaveform(trackId: string): string | null {
  const s = state();
  const t = s.lib.tracks.get(trackId);
  const a = s.analysis.tracks[trackId];
  return t && a && a.m === t.mtime ? a.w : null;
}

export async function setAnalysisSpeed(speed: ServerSettings['analysisSpeed']) {
  await updateSettings({ analysisSpeed: speed });
  await getLibrary();
  await startAnalysis();
  return getAnalysisStatus();
}

/** 분석이 끝날 때까지 기다린다. 테스트에서 쓴다. */
export async function waitForAnalysis() {
  await state().run?.done;
}

/** 스캔이 끝날 때까지 기다린다. 테스트와 CLI에서 쓴다. */
export async function waitForScan() {
  await state().scan;
}
