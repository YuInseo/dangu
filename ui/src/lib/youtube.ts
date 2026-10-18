/**
 * 유튜브. 보는 것은 유튜브 공식 플레이어(IFrame)가 하고, 소리 서버는 검색·영상 정보·댓글·채널(API 키)과
 * 구독·구독 피드(키 없이 채널 RSS)를 대신 묻는다. 영상이나 소리를 받거나 저장하지 않는다.
 */
import { useCallback, useEffect, useRef, useState } from 'react';
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

export type YtOrder = 'date' | 'relevance';

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
  next?: string;
  failed?: string[];
  /** 묵은 목록을 주고 뒤에서 새로 읽는 중 */
  refreshing?: boolean;
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

export interface YtFailure {
  title: string;
  detail?: string;
  /** 공식 플레이어의 오류 번호 */
  code?: number;
}

/** 영상 정보에서 이미 아는 것: 올린 사람이 유튜브 밖 재생을 꺼 두었다 */
export const EMBED_OFF: YtFailure = {
  title: '앱 안에서 재생할 수 없는 영상입니다',
  detail: '올린 사람이 유튜브 밖에서의 재생을 꺼 두었습니다.',
};

/** 공식 플레이어의 오류 번호 → 사람이 읽는 말 */
export function playerFailure(code: number): YtFailure {
  // 101·150: 올린 사람이나 음원 권리자가 유튜브 밖 재생을 막았다(음악 모음·공식 뮤직비디오에 흔하다).
  // 유튜브가 "봇이 아닌지 로그인해 확인"을 요구할 때도 같은 번호가 온다.
  if (code === 101 || code === 150)
    return { title: EMBED_OFF.title, detail: '올린 사람이나 음원 권리자가 유튜브 밖 재생을 막아 두었습니다.', code };
  if (code === 100) return { title: '영상을 찾을 수 없습니다', detail: '지워졌거나 비공개로 바뀌었습니다.', code };
  if (code === 153) return { title: '유튜브 플레이어가 이 앱을 확인하지 못했습니다', code };
  if (code === 2) return { title: '영상 주소가 이상합니다', code };
  return { title: '이 기기에서 재생할 수 없는 영상입니다', code };
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
  /** `order`: 'date'(새 것부터, 기본) 또는 'relevance'(관련순) */
  searchUrl: (text: string, page?: string, type?: string, order: YtOrder = 'date') =>
    `/api/youtube/search${q({ q: text, page, type, order, region: region() })}`,
  videoUrl: (id: string) => `/api/youtube/video/${encodeURIComponent(id)}`,
  commentsUrl: (id: string, order: 'relevance' | 'time', page?: string) =>
    `/api/youtube/comments/${encodeURIComponent(id)}${q({ order, page })}`,
  repliesUrl: (id: string, page?: string) => `/api/youtube/replies/${encodeURIComponent(id)}${q({ page })}`,
  channelUrl: (id: string) => `/api/youtube/channel/${encodeURIComponent(id)}`,
  channelVideosUrl: (id: string, page?: string) => `/api/youtube/channel/${encodeURIComponent(id)}/videos${q({ page })}`,
  channelPlaylistsUrl: (id: string, page?: string) => `/api/youtube/channel/${encodeURIComponent(id)}/playlists${q({ page })}`,
  playlistUrl: (id: string, page?: string) => `/api/youtube/playlist/${encodeURIComponent(id)}${q({ page })}`,
  /** `v`: 구독이 바뀌면 달라지는 표(기억해 둔 목록과 가르려고) */
  feedUrl: (page?: string, v?: string) => `/api/youtube/feed${q({ v, page })}`,
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

/** 구독한 채널들의 표. 구독이 바뀌면 달라진다. */
export function subsKey(list: YtSubscription[]): string {
  let h = 0;
  for (const id of list.map((x) => x.id).sort()) for (let i = 0; i < id.length; i++) h = (h * 31 + id.charCodeAt(i)) | 0;
  return `${list.length}.${(h >>> 0).toString(36)}`;
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

/** 이 세션 동안 기억한 목록. 뒤로 돌아오면 그 자리 그대로(스크롤도) 보인다. */
interface Kept {
  items: unknown[];
  next?: string;
  total?: number;
  head?: Record<string, unknown>;
  /** 첫 쪽을 받은 때 */
  at: number;
}

const kept = new Map<string, Kept>();

function keep(url: string, k: Kept) {
  kept.delete(url);
  kept.set(url, k);
  if (kept.size > 40) kept.delete(kept.keys().next().value!);
}

export interface PagedOptions<T> {
  /** 기억한 목록이 이만큼(밀리초) 묵었으면 첫 쪽을 다시 물어 새 것을 본다. 0이면 열 때마다. 없으면 묻지 않는다. */
  revalidate?: number;
  /** 이미 받은 첫 쪽(채널·재생목록 화면은 정보와 함께 받는다) */
  seed?: YtPage<T>;
  /** 응답에서 쪽을 꺼낸다(재생목록은 { playlist, videos }) */
  pick?: (raw: unknown) => YtPage<T>;
}

interface PagedState<T> {
  url: string | null;
  items: T[];
  next?: string;
  total?: number;
  /** 첫 쪽에 함께 온 것(구독 피드의 failed 같은) */
  head?: Record<string, unknown>;
  at: number;
  loading: boolean;
  error: unknown;
  moreError: unknown;
  /** 다시 물었더니 맨 위가 달라졌다. 아래를 보는 중이라 바로 바꾸지 않고 기다린다. */
  fresh?: YtPage<T>;
}

function headOf(raw: unknown): Record<string, unknown> | undefined {
  if (!raw || typeof raw !== 'object') return undefined;
  const rest: Record<string, unknown> = {};
  for (const [k, v] of Object.entries(raw)) if (k !== 'items' && k !== 'next' && k !== 'total') rest[k] = v;
  return Object.keys(rest).length ? rest : undefined;
}

/**
 * 다음 쪽을 이어 붙이는 목록. 받은 것은 이 세션 동안 기억해서 뒤로 돌아오면 곧바로 그대로 보이고,
 * `revalidate`면 첫 쪽을 다시 물어 맨 위에 새 것이 있는지 본다(맨 위를 보고 있으면 바로 바꾸고, 아래를
 * 보고 있으면 `fresh`로 알린다). 이어 붙일 때 이미 있는 것은 뺀다.
 */
export function usePaged<T extends { id: string }>(first: string | null, nextUrl: (token: string) => string, opts: PagedOptions<T> = {}) {
  const begin = (url: string | null): PagedState<T> => {
    const k = url ? kept.get(url) : undefined;
    const base = { url, loading: false, error: null, moreError: null };
    if (k) return { ...base, items: k.items as T[], next: k.next, total: k.total, head: k.head, at: k.at };
    if (url && opts.seed) return { ...base, items: opts.seed.items, next: opts.seed.next, total: opts.seed.total, at: Date.now() };
    return { ...base, items: [], at: 0, loading: !!url };
  };
  const [s, setS] = useState(() => begin(first));
  let cur = s;
  if (s.url !== first) {
    // 주소가 바뀌었다(다른 검색어·정렬): 그 주소의 기억으로, 없으면 빈 것으로 곧바로
    cur = begin(first);
    setS(cur);
  }

  const latest = useRef({ s: cur, nextUrl, pick: opts.pick });
  useEffect(() => {
    latest.current = { s, nextUrl, pick: opts.pick };
  });
  const read = useCallback((raw: unknown): YtPage<T> => {
    const pick = latest.current.pick;
    return pick ? pick(raw) : (raw as YtPage<T>);
  }, []);

  useEffect(() => {
    if (s.url && s.at) keep(s.url, { items: s.items, next: s.next, total: s.total, head: s.head, at: s.at });
  }, [s.url, s.items, s.next, s.total, s.head, s.at]);

  const had = cur.at;
  const revalidate = opts.revalidate;
  useEffect(() => {
    if (!first) return;
    if (had && (revalidate == null || Date.now() - had < revalidate)) return;
    let live = true;
    let timer: ReturnType<typeof setTimeout> | undefined;
    const load = (tries: number) => {
      fetchJson<unknown>(first)
        .then((raw) => {
          if (!live) return;
          const p = read(raw);
          setS((c) => {
            if (c.url !== first) return c;
            const done = { ...c, head: headOf(raw), at: Date.now(), loading: false, error: null };
            if (!c.items.length) return { ...done, items: p.items, next: p.next, total: p.total, fresh: undefined };
            if (c.items[0]?.id === p.items[0]?.id) return done;
            if (window.scrollY < 200) return { ...done, items: p.items, next: p.next, total: p.total, fresh: undefined };
            return { ...done, fresh: p };
          });
          // 서버가 뒤에서 새로 읽는 중이면 조금 뒤 한 번 더 묻는다
          if ((raw as { refreshing?: boolean }).refreshing && tries > 0) timer = setTimeout(() => load(tries - 1), 4000);
        })
        .catch((e: unknown) => {
          if (live) setS((c) => (c.url === first ? { ...c, loading: false, error: c.items.length ? c.error : e } : c));
        });
    };
    load(3);
    return () => {
      live = false;
      clearTimeout(timer);
    };
    // 첫 쪽은 주소가 바뀔 때만
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [first]);

  const busy = useRef(false);
  const more = useCallback(async () => {
    const { s: c, nextUrl: urlOf } = latest.current;
    if (!c.url || !c.next || busy.current) return;
    busy.current = true;
    const url = c.url;
    setS((x) => (x.url === url ? { ...x, loading: true, moreError: null } : x));
    try {
      const p = read(await fetchJson<unknown>(urlOf(c.next)));
      setS((x) => {
        if (x.url !== url) return x;
        const seen = new Set(x.items.map((i) => i.id));
        const added = p.items.filter((i) => !seen.has(i.id));
        // 새 것이 하나도 없는 쪽이 오면 거기서 멈춘다(같은 쪽을 끝없이 묻지 않게)
        return { ...x, items: [...x.items, ...added], next: added.length ? p.next : undefined, loading: false };
      });
    } catch (e) {
      setS((x) => (x.url === url ? { ...x, loading: false, moreError: e } : x));
    } finally {
      busy.current = false;
    }
  }, [read]);

  const showFresh = useCallback(() => {
    window.scrollTo({ top: 0, behavior: 'smooth' });
    setS((x) => (x.fresh ? { ...x, items: x.fresh.items, next: x.fresh.next, total: x.fresh.total, fresh: undefined } : x));
  }, []);

  return {
    items: cur.items,
    next: cur.next,
    total: cur.total,
    head: cur.head,
    loading: cur.loading,
    error: cur.error,
    moreError: cur.moreError,
    more,
    fresh: !!cur.fresh,
    showFresh,
  };
}

export type Paged<T extends { id: string }> = ReturnType<typeof usePaged<T>>;

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
