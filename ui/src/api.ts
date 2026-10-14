/**
 * 서버에서 데이터를 받는 곳.
 *
 * 받은 것은 주소별로 기억해 두었다가, 같은 쪽으로 돌아오면 곧바로 그리고 뒤에서 새로 받는다.
 * 라이브러리가 바뀌면(스캔이 끝나거나 분석 결과가 쌓이면) 서버가 알려 주고, 그때 다시 받는다.
 */
import { useEffect, useState, useSyncExternalStore } from 'react';

export class ApiError extends Error {
  constructor(
    message: string,
    readonly status: number,
  ) {
    super(message);
  }
}

export async function fetchJson<T>(url: string, init?: RequestInit): Promise<T> {
  const res = await fetch(url, { cache: 'no-store', ...init });
  if (res.status === 401 && !location.pathname.startsWith('/login')) {
    location.href = `/login?next=${encodeURIComponent(location.pathname + location.search)}`;
  }
  if (!res.ok) {
    let msg = `${res.status}`;
    try {
      msg = ((await res.json()) as { error?: string }).error ?? msg;
    } catch {
      // 본문이 JSON이 아니다
    }
    throw new ApiError(msg, res.status);
  }
  return (await res.json()) as T;
}

const cache = new Map<string, unknown>();
let version = 0;
const listeners = new Set<() => void>();

function bump() {
  version++;
  for (const l of listeners) l();
}

/** 라이브러리 세대가 바뀌었다. 모든 쪽이 데이터를 다시 받는다. */
let lastGeneration = -1;
export function noteGeneration(g: number) {
  if (lastGeneration !== -1 && g !== lastGeneration) bump();
  lastGeneration = g;
}

export function refreshData() {
  bump();
}

function useVersion() {
  return useSyncExternalStore(
    (l) => {
      listeners.add(l);
      return () => listeners.delete(l);
    },
    () => version,
  );
}

export interface ApiState<T> {
  data: T | undefined;
  error: ApiError | undefined;
  loading: boolean;
}

export function useApi<T>(url: string | null): ApiState<T> {
  const v = useVersion();
  const [state, setState] = useState<{ url: string | null; data?: T; error?: ApiError }>(() => ({
    url,
    data: url ? (cache.get(url) as T | undefined) : undefined,
  }));

  useEffect(() => {
    if (!url) return;
    let alive = true;
    fetchJson<T>(url)
      .then((data) => {
        cache.set(url, data);
        if (cache.size > 300) cache.delete(cache.keys().next().value!);
        if (alive) setState({ url, data });
      })
      .catch((e: unknown) => {
        const err = e instanceof ApiError ? e : new ApiError(e instanceof Error ? e.message : String(e), 0);
        if (alive) setState((s) => ({ url, data: s.url === url ? s.data : undefined, error: err }));
      });
    return () => {
      alive = false;
    };
  }, [url, v]);

  const same = state.url === url;
  const data = same ? (state.data ?? (url ? (cache.get(url) as T | undefined) : undefined)) : url ? (cache.get(url) as T | undefined) : undefined;
  const error = same ? state.error : undefined;
  return { data, error, loading: !!url && data === undefined && !error };
}

/** 문서 제목. "앨범 · Sori" */
export function useTitle(title: string | undefined) {
  useEffect(() => {
    document.title = title ? `${title} · Sori` : 'Sori';
  }, [title]);
}
