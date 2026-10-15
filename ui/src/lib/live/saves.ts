/**
 * 라이브 아카이브 공연을 라이브러리에 저장하기. 받는 일은 소리 서버가 하고(한 번에 한 공연씩),
 * 화면은 상태를 보여 준다. 아티스트가 내려받기를 허락한 공연만 받는다.
 */
import { useCallback, useEffect, useRef, useState } from 'react';
import { ApiError, fetchJson } from '@/api';
import { bytes } from '../time';

export type SaveState = 'queued' | 'saving' | 'done' | 'failed' | 'cancelled';

export interface SaveJob {
  id: string;
  band?: string;
  album?: string;
  state: SaveState;
  filesDone?: number;
  filesTotal?: number;
  bytesDone?: number;
  bytesTotal?: number | null;
  error?: string | null;
  /** 저장한 폴더 */
  dir?: string;
  /** 라이브러리가 다시 읽은 뒤의 앨범 */
  albumId?: string | null;
}

const post = <T>(url: string, body?: unknown) =>
  fetchJson<T>(
    url,
    body === undefined
      ? { method: 'POST' }
      : { method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify(body) },
  );

const one = (id: string) => `/api/live/saves/${encodeURIComponent(id)}`;

export const savesApi = {
  list: () => fetchJson<{ jobs: SaveJob[] }>('/api/live/saves'),
  /** 저장하지 않은 공연이면 null */
  get: async (id: string) => {
    try {
      return await fetchJson<SaveJob>(one(id));
    } catch (e) {
      if (e instanceof ApiError && e.status === 404) return null;
      throw e;
    }
  },
  save: (id: string) => post<SaveJob>('/api/live/save', { id }),
  cancel: (id: string) => post<SaveJob>(`${one(id)}/cancel`),
  dismiss: (id: string) => fetch(one(id), { method: 'DELETE' }).then(() => undefined),
  /** 붙여 넣은 링크 → 공연 ID(라이브 음악 아카이브의 공연만) */
  resolve: (url: string) => post<{ id: string; saveable: boolean; saveBlock?: string }>('/api/live/resolve', { url }),
};

export const isActive = (j: SaveJob | null | undefined) => j?.state === 'queued' || j?.state === 'saving';

/** 0–1. 크기를 모르면 곡 수로. */
export function saveFraction(j: SaveJob): number {
  if (j.state === 'done') return 1;
  if (j.bytesTotal) return Math.min(1, (j.bytesDone ?? 0) / j.bytesTotal);
  return j.filesTotal ? Math.min(1, (j.filesDone ?? 0) / j.filesTotal) : 0;
}

/** "저장 중 3/12곡 · 412 MB / 1.2 GB" */
export function saveText(j: SaveJob): string {
  switch (j.state) {
    case 'queued':
      return '저장 대기 중';
    case 'saving': {
      const files = j.filesTotal ? ` ${Math.min((j.filesDone ?? 0) + 1, j.filesTotal)}/${j.filesTotal}곡` : '';
      const size = j.bytesTotal ? ` · ${bytes(j.bytesDone ?? 0)} / ${bytes(j.bytesTotal)}` : '';
      return `저장 중${files}${size}`;
    }
    case 'done':
      return '라이브러리에 저장했습니다';
    case 'failed':
      return j.error ? `저장하지 못했습니다: ${j.error}` : '저장하지 못했습니다';
    case 'cancelled':
      return '취소했습니다';
  }
}

/** 공연 하나의 저장 상태. 받는 중이면 1초마다, 끝났는데 아직 앨범이 없으면(라이브러리를 읽는 중) 2초마다 묻는다. */
export function useSaveJob(id: string) {
  const [job, setJob] = useState<SaveJob | null>(null);
  const timer = useRef<ReturnType<typeof setTimeout>>(undefined);
  const refresh = useCallback(() => {
    savesApi
      .get(id)
      .then(setJob)
      .catch(() => {});
  }, [id]);
  useEffect(() => {
    setJob(null);
    refresh();
  }, [refresh]);
  const waiting = isActive(job) || (job?.state === 'done' && !job.albumId);
  useEffect(() => {
    clearTimeout(timer.current);
    if (waiting) timer.current = setTimeout(refresh, isActive(job) ? 1000 : 2000);
    return () => clearTimeout(timer.current);
  }, [job, waiting, refresh]);
  return { job, setJob, refresh };
}

/** 받기 목록. 받는 것이 있으면 1초마다. */
export function useSaveJobs() {
  const [jobs, setJobs] = useState<SaveJob[]>([]);
  const timer = useRef<ReturnType<typeof setTimeout>>(undefined);
  const refresh = useCallback(() => {
    savesApi
      .list()
      .then((r) => setJobs(r.jobs))
      .catch(() => {});
  }, []);
  useEffect(() => {
    refresh();
  }, [refresh]);
  const busy = jobs.some((j) => isActive(j) || (j.state === 'done' && !j.albumId));
  useEffect(() => {
    clearTimeout(timer.current);
    if (busy) timer.current = setTimeout(refresh, 1000);
    return () => clearTimeout(timer.current);
  }, [jobs, busy, refresh]);
  return { jobs, setJobs, refresh };
}
