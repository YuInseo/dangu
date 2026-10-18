/**
 * 자동 업데이트. 소리 서버(앱 안)가 릴리스의 latest.json을 보고 새 빌드를 뒤에서 받아 두며,
 * 화면은 상태를 보여 주고 "업데이트"를 누르면 설치를 맡긴다.
 */
import { useCallback, useEffect, useRef, useState } from 'react';
import { fetchJson } from '@/api';

export type UpdateDownload =
  | { state: 'idle' }
  | { state: 'downloading'; done: number; total: number }
  | { state: 'ready' }
  | { state: 'installing' }
  | { state: 'failed'; error: string };

export interface UpdateStatus {
  /** 이 빌드(커밋). 직접 빌드한 것이면 null */
  build: string | null;
  buildTime: number;
  /** 이 기기에서 업데이트할 수 있는가(앱 안이고 CI가 만든 빌드) */
  supported: boolean;
  auto: boolean;
  checkedAt: number | null;
  error: string | null;
  available: boolean;
  latest: { build: string; time: number; notes: string; size: number | null } | null;
  download: UpdateDownload;
}

const post = (url: string, body?: unknown) =>
  fetchJson<UpdateStatus>(
    url,
    body === undefined
      ? { method: 'POST' }
      : { method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify(body) },
  );

export const updateApi = {
  status: () => fetchJson<UpdateStatus>('/api/update'),
  check: () => post('/api/update/check'),
  install: () => post('/api/update/install'),
  setAuto: (on: boolean) => post('/api/update/auto', { on }),
};

export function shortBuild(b: string | null | undefined) {
  return b ? b.slice(0, 7) : '';
}

/** 받는 중·설치 중이면 1초마다, 아니면 `idleMs`마다 다시 묻는다. */
export function useUpdate(idleMs = 30 * 60_000) {
  const [status, setStatus] = useState<UpdateStatus | null>(null);
  const timer = useRef<ReturnType<typeof setTimeout>>(undefined);
  const refresh = useCallback(() => {
    updateApi
      .status()
      .then(setStatus)
      .catch(() => {});
  }, []);
  useEffect(() => {
    refresh();
  }, [refresh]);
  const busy = status?.download.state === 'downloading' || status?.download.state === 'installing';
  // 앱을 다시 열면(뒤에 있다가 앞으로 오면) 30분이 지났을 때 새 버전을 바로 묻는다.
  useEffect(() => {
    const onShow = () => {
      if (document.visibilityState !== 'visible' || !status?.supported) return;
      if (status.checkedAt && Date.now() - status.checkedAt < 30 * 60_000) return;
      updateApi
        .check()
        .then(setStatus)
        .catch(() => {});
    };
    document.addEventListener('visibilitychange', onShow);
    return () => document.removeEventListener('visibilitychange', onShow);
  }, [status]);
  useEffect(() => {
    clearTimeout(timer.current);
    timer.current = setTimeout(refresh, busy ? 1000 : idleMs);
    return () => clearTimeout(timer.current);
  }, [status, busy, idleMs, refresh]);
  return { status, setStatus, refresh };
}

/** 설치를 맡긴다. 아직 받지 않았으면 받는 것부터(상태는 useUpdate가 따라간다). */
export async function startInstall(setStatus: (s: UpdateStatus) => void) {
  setStatus(await updateApi.install());
}

export function progressText(d: UpdateDownload): string | null {
  switch (d.state) {
    case 'downloading':
      return d.total ? `받는 중 ${Math.floor((d.done / d.total) * 100)}%` : '받는 중';
    case 'installing':
      return '설치를 시작합니다';
    case 'failed':
      return d.error;
    default:
      return null;
  }
}
