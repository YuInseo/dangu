/**
 * 유튜브. 보는 것은 유튜브 공식 플레이어(IFrame)가 하고, 소리 서버는 검색·영상 정보·댓글·채널(API 키)과
 * 구독·구독 피드(키 없이 채널 RSS)를 대신 묻는다. 영상이나 소리를 받거나 저장하지 않는다.
 */
import { useCallback, useEffect, useState } from 'react';
import { ApiError, fetchJson } from '@/api';

export interface YtVideo {
  id: string;
  title: string;
  channelId?: string;
  channelTitle: string;
  published?: string;
  thumb?: string;
  duration?: number;
  views?: number;
  likes?: number;
  comments?: number;
  description?: string;
  live?: 'live' | 'upcoming';
  short?: boolean;
  embeddable?: boolean;
}

export interface YtChannel {
  id: string;
  title: string;
  handle?: string;
  description?: string;
  thumb?: string;
  banner?: string;
  subscribers?: number;
  videos?: number;
  views?: number;
  uploads?: string;
}

export interface YtPlaylist {
  id: string;
  title: string;
  channelId?: string;
  channelTitle: string;
  thumb?: string;
  count?: number;
  description?: string;
}

export interface YtComment {
  id: string;
  author: string;
  authorChannelId?: string;
  authorAvatar?: string;
  text: string;
  likes: number;
  published?: string;
  edited?: boolean;
  replies: number;
  replyPreview?: YtComment[];
}

export type YtSearchItem = ({ kind: 'video' } & YtVideo) | ({ kind: 'channel' } & YtChannel) | ({ kind: 'playlist' } & YtPlaylist);

export interface YtPage<T> {
  items: T[];
  next?: string;
  total?: number;
}

export interface YtVideoDetail {
  video: YtVideo;
  channel?: YtChannel;
  /** 키 없이 읽었다(조회수·설명·댓글 없음) */
  limited: boolean;
  subscribed?: boolean;
}

export interface YtChannelDetail {
  channel: YtChannel;
  videos: YtPage<YtVideo>;
  limited: boolean;
  subscribed: boolean;
}

export interface YtPlaylistDetail {
  playlist: YtPlaylist;
  videos: YtPage<YtVideo>;
}

export interface YtSubscription {
  id: string;
  title: string;
  thumb?: string;
  addedAt: number;
}

export interface YtFeed {
  items: YtVideo[];
  next?: number;
  failed?: string[];
}

export interface YtStatus {
  hasKey: boolean;
  keyHint?: string | null;
  subscriptions: number;
}

/** 키가 있어야 하는 것을 키 없이 불렀다 */
export function needsKey(e: unknown) {
  return e instanceof ApiError && e.status === 412;
}

const json = (method: string, body?: unknown): RequestInit =>
  body === undefined ? { method } : { method, headers: { 'content-type': 'application/json' }, body: JSON.stringify(body) };

/** 기기의 나라(인기 차트·검색 지역) */
export function region(): string {
  const lang = typeof navigator !== 'undefined' ? navigator.language : 'ko-KR';
  const m = /-([A-Za-z]{2})\b/.exec(lang);
  return (m?.[1] ?? 'KR').toUpperCase();
}

const q = (o: Record<string, string | undefined>) => {
  const s = new URLSearchParams();
  for (const [k, v] of Object.entries(o)) if (v) s.set(k, v);
  const str = s.toString();
  return str ? `?${str}` : '';
};

export const yt = {
  status: () => fetchJson<YtStatus>('/api/youtube'),
  setKey: (key: string) => fetchJson<YtStatus>('/api/youtube/key', json('POST', { key })),
  clearKey: () => fetchJson<YtStatus>('/api/youtube/key', json('DELETE')),
  popularUrl: (page?: string) => `/api/youtube/popular${q({ region: region(), page })}`,
  searchUrl: (text: string, page?: string, type?: string) => `/api/youtube/search${q({ q: text, page, type, region: region() })}`,
  videoUrl: (id: string) => `/api/youtube/video/${encodeURIComponent(id)}`,
  commentsUrl: (id: string, order: 'relevance' | 'time', page?: string) =>
    `/api/youtube/comments/${encodeURIComponent(id)}${q({ order, page })}`,
  repliesUrl: (id: string, page?: string) => `/api/youtube/replies/${encodeURIComponent(id)}${q({ page })}`,
  channelUrl: (id: string) => `/api/youtube/channel/${encodeURIComponent(id)}`,
  channelVideosUrl: (id: string, page?: string) => `/api/youtube/channel/${encodeURIComponent(id)}/videos${q({ page })}`,
  channelPlaylistsUrl: (id: string, page?: string) => `/api/youtube/channel/${encodeURIComponent(id)}/playlists${q({ page })}`,
  playlistUrl: (id: string, page?: string) => `/api/youtube/playlist/${encodeURIComponent(id)}${q({ page })}`,
  feedUrl: (page?: number) => `/api/youtube/feed${page ? `?page=${page}` : ''}`,
  resolve: (url: string) =>
    fetchJson<{ kind: 'video' | 'channel' | 'playlist' | 'handle'; id: string }>('/api/youtube/resolve', json('POST', { url })),
  subscriptions: () => fetchJson<YtSubscription[]>('/api/youtube/subscriptions'),
  subscribe: (c: { id: string; title: string; thumb?: string }) =>
    fetchJson<{ subscribed: boolean; count: number }>('/api/youtube/subscriptions', json('POST', c)),
  unsubscribe: (id: string) =>
    fetchJson<{ subscribed: boolean; count: number }>(`/api/youtube/subscriptions/${encodeURIComponent(id)}`, json('DELETE')),
  importCsv: (csv: string) =>
    fetchJson<{ found: number; added: number; count: number }>('/api/youtube/subscriptions/import', {
      method: 'POST',
      headers: { 'content-type': 'text/csv' },
      body: csv,
    }),
};

// ---------------------------------------------------------------------------
// 키와 구독 상태(여러 화면이 같이 본다)

type Listener = () => void;
let status: YtStatus | null = null;
let subs: YtSubscription[] | null = null;
const listeners = new Set<Listener>();
const emit = () => listeners.forEach((l) => l());

export async function refreshYt() {
  try {
    const [s, list] = await Promise.all([yt.status(), yt.subscriptions()]);
    status = s;
    subs = list;
  } catch {
    // 서버가 아직 없다
  }
  emit();
}

export function noteKey(s: YtStatus) {
  status = { ...(status ?? { subscriptions: 0 }), ...s };
  emit();
}

function useStore() {
  const [, force] = useState(0);
  useEffect(() => {
    const l = () => force((n) => n + 1);
    listeners.add(l);
    if (!status) void refreshYt();
    return () => {
      listeners.delete(l);
    };
  }, []);
}

export function useYtStatus(): YtStatus | null {
  useStore();
  return status;
}

export function useSubscriptions(): YtSubscription[] | null {
  useStore();
  return subs;
}

export function isSubscribed(id: string | undefined) {
  return !!id && !!subs?.some((s) => s.id === id);
}

/** 구독/구독 끊기. 화면은 바로 바뀌고, 실패하면 되돌린다. */
export async function setSubscribed(c: { id: string; title: string; thumb?: string }, on: boolean) {
  const before = subs;
  subs = on
    ? [{ id: c.id, title: c.title, thumb: c.thumb, addedAt: Date.now() }, ...(subs ?? []).filter((s) => s.id !== c.id)]
    : (subs ?? []).filter((s) => s.id !== c.id);
  emit();
  try {
    if (on) await yt.subscribe(c);
    else await yt.unsubscribe(c.id);
    await refreshYt();
  } catch (e) {
    subs = before;
    emit();
    throw e;
  }
}

/** 다음 쪽을 이어 붙이는 목록 */
export function usePaged<T>(first: string | null, nextUrl: (token: string) => string) {
  const [items, setItems] = useState<T[]>([]);
  const [next, setNext] = useState<string | undefined>();
  const [error, setError] = useState<unknown>(null);
  const [loading, setLoading] = useState(false);
  const [total, setTotal] = useState<number | undefined>();
  useEffect(() => {
    setItems([]);
    setNext(undefined);
    setError(null);
    if (!first) return;
    let live = true;
    setLoading(true);
    fetchJson<YtPage<T>>(first)
      .then((p) => {
        if (!live) return;
        setItems(p.items);
        setNext(p.next);
        setTotal(p.total);
      })
      .catch((e) => live && setError(e))
      .finally(() => live && setLoading(false));
    return () => {
      live = false;
    };
  }, [first]);
  const more = useCallback(async () => {
    if (!next || loading) return;
    setLoading(true);
    try {
      const p = await fetchJson<YtPage<T>>(nextUrl(next));
      setItems((a) => [...a, ...p.items]);
      setNext(p.next);
    } catch (e) {
      setError(e);
    } finally {
      setLoading(false);
    }
  }, [next, loading, nextUrl]);
  return { items, next, more, error, loading, total };
}

// ---------------------------------------------------------------------------
// 글자

/** 1234 → "1.2천", 12345 → "1.2만", 123456789 → "1.2억" */
export function compact(n: number): string {
  const f = (v: number) =>
    v >= 100 ? Math.round(v).toString() : v >= 10 ? Math.round(v).toString() : (Math.round(v * 10) / 10).toString();
  if (n >= 1e8) return `${f(n / 1e8)}억`;
  if (n >= 1e4) return `${f(n / 1e4)}만`;
  if (n >= 1e3) return `${f(n / 1e3)}천`;
  return String(n);
}

export const viewsText = (n?: number) => (n == null ? null : `조회수 ${compact(n)}회`);
export const subsText = (n?: number) => (n == null ? null : `구독자 ${compact(n)}명`);

/** "3분 전", "2일 전", "3주 전", "5개월 전", "2년 전" */
export function agoIso(iso?: string, now = Date.now()): string | null {
  if (!iso) return null;
  const t = Date.parse(iso);
  if (!Number.isFinite(t)) return null;
  const s = Math.max(0, (now - t) / 1000);
  if (s < 60) return '방금';
  if (s < 3600) return `${Math.floor(s / 60)}분 전`;
  if (s < 86400) return `${Math.floor(s / 3600)}시간 전`;
  const d = Math.floor(s / 86400);
  if (d < 7) return `${d}일 전`;
  if (d < 30) return `${Math.floor(d / 7)}주 전`;
  if (d < 365) return `${Math.floor(d / 30)}개월 전`;
  return `${Math.floor(d / 365)}년 전`;
}

/** 길이 배지: "3:21", "1:02:03" */
export function length(sec?: number): string | null {
  if (!sec || !Number.isFinite(sec)) return null;
  const s = Math.round(sec);
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  const ss = String(s % 60).padStart(2, '0');
  return h ? `${h}:${String(m).padStart(2, '0')}:${ss}` : `${m}:${ss}`;
}

/** 설명·댓글의 주소와 시각(1:23)을 누를 수 있게 나눈다. */
export type Piece = { t: 'text'; v: string } | { t: 'url'; v: string } | { t: 'time'; v: string; sec: number };

export function linkify(text: string): Piece[] {
  const out: Piece[] = [];
  const re = /(https?:\/\/[^\s<>"]+)|(?<![\d:])((?:\d{1,2}:)?\d{1,2}:\d{2})(?![\d:])/g;
  let last = 0;
  for (const m of text.matchAll(re)) {
    const i = m.index ?? 0;
    if (i > last) out.push({ t: 'text', v: text.slice(last, i) });
    if (m[1]) {
      // 문장 끝의 마침표·괄호는 주소가 아니다
      const url = m[1].replace(/[).,!?]+$/, '');
      out.push({ t: 'url', v: url });
      if (url.length < m[1].length) out.push({ t: 'text', v: m[1].slice(url.length) });
    } else {
      const parts = m[2]!.split(':').map(Number);
      const sec = parts.reduce((a, b) => a * 60 + b, 0);
      out.push({ t: 'time', v: m[2]!, sec });
    }
    last = i + m[0].length;
  }
  if (last < text.length) out.push({ t: 'text', v: text.slice(last) });
  return out;
}

/** 우리 앱 안의 주소로 바꿀 수 있는 유튜브 링크면 그 주소 */
export function internalHref(url: string): string | null {
  const m = /^https?:\/\/(?:www\.|m\.)?(?:youtube\.com\/(?:watch\?(?:.*&)?v=|shorts\/|live\/)|youtu\.be\/)([\w-]{11})/.exec(url);
  if (m) return `/youtube/watch/${m[1]}`;
  const c = /^https?:\/\/(?:www\.|m\.)?youtube\.com\/channel\/(UC[\w-]{22})/.exec(url);
  if (c) return `/youtube/channel/${c[1]}`;
  return null;
}
