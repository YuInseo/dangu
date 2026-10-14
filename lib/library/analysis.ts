/**
 * 백그라운드 오디오 분석.
 *
 * 곡마다 ffmpeg를 한 번 돌려 EBU R128 통합 라우드니스, 트루 피크, 라우드니스 범위, 그리고
 * 진행 막대에 그릴 파형을 얻는다. 볼륨 레벨링은 이 값으로 게인을 정하고, 트루 피크로 클리핑을
 * 막는다. 라이브러리가 크면 오래 걸리므로 낮은 우선순위로 조금씩 한다.
 */
import { spawn } from 'node:child_process';
import fs from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { config } from '../config';
import { pool } from '../pool';
import type { StoredTrack } from './types';

export interface AnalysisEntry {
  /** 분석한 파일의 수정 시각. 바뀌면 다시 분석한다. */
  m: number;
  /** 통합 라우드니스, LUFS */
  i: number;
  /** 트루 피크, dBTP */
  p: number;
  /** 라우드니스 범위, LU */
  r: number;
  /** 파형: WAVE_BINS개의 0–255, base64 */
  w: string;
}

export interface AnalysisFile {
  v: 1;
  tracks: Record<string, AnalysisEntry>;
}

export const WAVE_BINS = 200;

const file = () => path.join(config.dataDir, 'analysis.json');

export async function readAnalysis(): Promise<AnalysisFile> {
  try {
    const a = JSON.parse(await fs.readFile(file(), 'utf8')) as AnalysisFile;
    if (a.v === 1 && a.tracks) return a;
  } catch {
    // 처음
  }
  return { v: 1, tracks: {} };
}

export async function writeAnalysis(a: AnalysisFile) {
  await fs.mkdir(config.dataDir, { recursive: true });
  const tmp = `${file()}.${process.pid}.tmp`;
  await fs.writeFile(tmp, JSON.stringify(a));
  await fs.rename(tmp, file());
}

/** ebur128 요약에서 숫자를 꺼낸다. "-inf"는 무음. */
export function parseEbur128(stderr: string): { i: number; p: number; r: number } | null {
  const summary = stderr.slice(stderr.lastIndexOf('Summary:'));
  const num = (re: RegExp) => {
    const m = re.exec(summary);
    if (!m) return null;
    return m[1] === '-inf' ? -Infinity : Number(m[1]);
  };
  const i = num(/I:\s+(-?inf|-?[\d.]+)\s+LUFS/);
  const r = num(/LRA:\s+(-?inf|-?[\d.]+)\s+LU/);
  const p = num(/Peak:\s+(-?inf|-?[\d.]+)\s+dBFS/);
  if (i == null || p == null) return null;
  return { i: Number.isFinite(i) ? i : -70, p: Number.isFinite(p) ? p : -120, r: r != null && Number.isFinite(r) ? r : 0 };
}

/** 4kHz 모노 16비트 PCM → 구간별 RMS를 0–255로. 조용한 곳도 보이게 dB 눈금으로 편다. */
export function waveform(pcm: Buffer, bins = WAVE_BINS): Uint8Array {
  const n = Math.floor(pcm.length / 2);
  const out = new Uint8Array(bins);
  if (!n) return out;
  for (let b = 0; b < bins; b++) {
    const s = Math.floor((b * n) / bins);
    const e = Math.max(s + 1, Math.floor(((b + 1) * n) / bins));
    let sum = 0;
    for (let i = s; i < e; i++) {
      const v = pcm.readInt16LE(i * 2) / 32768;
      sum += v * v;
    }
    const rms = Math.sqrt(sum / (e - s));
    const db = 20 * Math.log10(Math.max(rms, 1e-6));
    out[b] = Math.round(Math.min(1, Math.max(0, (db + 45) / 45)) * 255);
  }
  return out;
}

export function analyzeFile(abs: string, lowPriority: boolean): Promise<Omit<AnalysisEntry, 'm'> | null> {
  return new Promise((resolve) => {
    const args = [
      '-hide_banner',
      '-nostdin',
      '-nostats',
      '-i',
      abs,
      '-filter_complex',
      '[0:a:0]asplit=2[a][b];[a]ebur128=peak=true:framelog=quiet,anullsink;[b]aformat=sample_fmts=s16:channel_layouts=mono,aresample=4000[w]',
      '-map',
      '[w]',
      '-f',
      's16le',
      'pipe:1',
    ];
    let proc;
    try {
      proc = spawn(/* turbopackIgnore: true */ config.ffmpeg, args, { stdio: ['ignore', 'pipe', 'pipe'] });
    } catch {
      resolve(null);
      return;
    }
    if (lowPriority && proc.pid) {
      try {
        os.setPriority(proc.pid, 19);
      } catch {
        // 권한이 없으면 그냥 돈다
      }
    }
    const chunks: Buffer[] = [];
    let err = '';
    proc.stdout.on('data', (d: Buffer) => chunks.push(d));
    proc.stderr.on('data', (d: Buffer) => {
      err += d.toString();
      if (err.length > 1 << 20) err = err.slice(-(1 << 16));
    });
    proc.on('error', () => resolve(null));
    proc.on('exit', (code) => {
      const loud = code === 0 ? parseEbur128(err) : null;
      if (!loud) return resolve(null);
      resolve({ ...loud, w: Buffer.from(waveform(Buffer.concat(chunks))).toString('base64') });
    });
  });
}

/** 아직 분석하지 않았거나 파일이 바뀐 곡. */
export function pendingTracks(tracks: StoredTrack[], a: AnalysisFile): StoredTrack[] {
  return tracks.filter((t) => a.tracks[t.id]?.m !== t.mtime);
}

export interface AnalysisRun {
  stop(): void;
  done: Promise<void>;
}

/**
 * 분석을 돌린다. 결과가 몇 개 쌓일 때마다 저장하고 `onBatch`를 부른다(라이브러리가 레벨링 값을
 * 갱신한다). `stop()`하면 진행 중인 곡까지만 끝낸다.
 */
export function runAnalysis(opts: {
  root: string;
  tracks: StoredTrack[];
  analysis: AnalysisFile;
  speed: 'throttled' | 'fast';
  onProgress: (done: number) => void;
  onBatch: () => void;
}): AnalysisRun {
  let stopped = false;
  const workers = opts.speed === 'fast' ? Math.max(1, Math.min(4, Math.floor(os.cpus().length / 2))) : 1;
  let done = 0;
  let sinceSave = 0;
  let lastSave = Date.now();

  const save = async () => {
    sinceSave = 0;
    lastSave = Date.now();
    await writeAnalysis(opts.analysis).catch(() => {});
    opts.onBatch();
  };

  const promise = (async () => {
    // 지워진 곡의 분석은 버린다
    const live = new Set(opts.tracks.map((t) => t.id));
    for (const id of Object.keys(opts.analysis.tracks)) if (!live.has(id)) delete opts.analysis.tracks[id];

    const queue = pendingTracks(opts.tracks, opts.analysis);
    await pool(queue, workers, async (t) => {
      if (stopped) return;
      const r = await analyzeFile(path.join(opts.root, t.path), opts.speed === 'throttled');
      if (r) opts.analysis.tracks[t.id] = { m: t.mtime, ...r };
      done++;
      sinceSave++;
      opts.onProgress(done);
      if (sinceSave >= 20 || Date.now() - lastSave > 15_000) await save();
      // 느리게: 곡 사이에 쉰다. 재생 중인 기기에 부담을 주지 않으려는 것.
      if (opts.speed === 'throttled' && !stopped) await new Promise((r) => setTimeout(r, 250));
    });
    if (sinceSave > 0 || queue.length === 0) await save();
  })();

  return {
    stop() {
      stopped = true;
    },
    done: promise,
  };
}
