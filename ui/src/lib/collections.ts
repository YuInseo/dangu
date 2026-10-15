/**
 * 좋아요와 플레이리스트. 서버에 두므로 같은 소리에 연결한 화면(휴대폰, PC의 브라우저)이 모두 같은 것을 본다.
 *
 * 좋아요는 ID 묶음만 들고 있다가(하트를 그릴 때 곧바로 안다) 누르면 먼저 바꿔 그리고 서버에 알린다.
 * 다른 화면에서 바꾸면 서버가 /api/events로 알려 주고(`collections` 세대), 그때 다시 받는다.
 */
import { useSyncExternalStore } from 'react';
import { fetchJson, refreshData } from '@/api';
import type { CoverRef, Track } from './library/types';
import type { PlayableTrack } from './player/types';

export type LikeKind = 'tracks' | 'albums';

export interface PlaylistCover {
  albumId: string;
  cover?: CoverRef;
  title: string;
  artist: string;
}

export interface PlaylistSummary {
  id: string;
  name: string;
  created: number;
  updated: number;
  /** 라이브러리에서 찾은 곡 수 */
  count: number;
  duration: number;
  /** 서로 다른 앨범의 표지 넷까지(모자이크) */
  covers: PlaylistCover[];
}

export interface PlaylistDetail extends PlaylistSummary {
  /** 적어 둔 곡 ID 전부(라이브러리에서 사라진 것도). 고칠 때는 이것을 바꿔 보낸다. */
  ids: string[];
  /** 찾은 곡만, 적어 둔 순서대로 */
  tracks: Track[];
  missing: number;
}

/** 내 라이브러리의 곡이면 그 ID. 라이브 아카이브 곡은 좋아요·플레이리스트에 넣지 않는다. */
export function libraryId(t: PlayableTrack): string | undefined {
  return t.source === 'library' && t.key.startsWith('lib:') ? t.key.slice(4) : undefined;
}

export function libraryIds(tracks: PlayableTrack[]): string[] {
  return tracks.map(libraryId).filter((id): id is string => !!id);
}

// -- 좋아요 ---------------------------------------------------------------

const liked: Record<LikeKind, Set<string>> = { tracks: new Set(), albums: new Set() };
/** 누르고 서버의 답을 기다리는 것. 그사이 받은 목록이 덮어쓰지 않게 한다. */
const pending = new Map<string, boolean>();
const listeners = new Set<() => void>();
let loaded = false;
let loading: Promise<void> | null = null;
/** 받는 중에 또 바뀌었다는 알림이 오면, 다 받은 뒤 한 번 더 받는다. */
let again = false;

function emit() {
  for (const l of listeners) l();
}

function put(kind: LikeKind, id: string, on: boolean) {
  if (on) liked[kind].add(id);
  else liked[kind].delete(id);
}

function subscribe(l: () => void) {
  listeners.add(l);
  if (!loaded) void refreshFavorites();
  return () => {
    listeners.delete(l);
  };
}

/** 서버에서 좋아요 목록을 다시 받는다. */
export function refreshFavorites(): Promise<void> {
  if (loading) {
    again = true;
    return loading;
  }
  loading = fetchJson<{ tracks: string[]; albums: string[] }>('/api/favorites/ids')
    .then((d) => {
      liked.tracks = new Set(d.tracks);
      liked.albums = new Set(d.albums);
      for (const [key, on] of pending) {
        const [kind, id] = key.split(':', 2) as [LikeKind, string];
        put(kind, id, on);
      }
      loaded = true;
      emit();
    })
    .catch(() => {
      // 다음 알림이나 다음 화면에서 다시
    })
    .finally(() => {
      loading = null;
      if (again) {
        again = false;
        void refreshFavorites();
      }
    });
  return loading;
}

export function isLiked(kind: LikeKind, id: string | undefined): boolean {
  return !!id && liked[kind].has(id);
}

export function useLiked(kind: LikeKind, id: string | undefined): boolean {
  return useSyncExternalStore(
    subscribe,
    () => isLiked(kind, id),
    () => false,
  );
}

export function useLikedCount(kind: LikeKind): number {
  return useSyncExternalStore(
    subscribe,
    () => liked[kind].size,
    () => 0,
  );
}

/** 바뀐 것을 먼저 그리고 서버에 알린다. 실패하면 되돌리고 오류를 던진다. */
export async function setLiked(kind: LikeKind, id: string, on: boolean): Promise<void> {
  const key = `${kind}:${id}`;
  const before = liked[kind].has(id);
  pending.set(key, on);
  put(kind, id, on);
  emit();
  try {
    await fetchJson(`/api/favorites/${kind}/${encodeURIComponent(id)}`, { method: on ? 'PUT' : 'DELETE' });
  } catch (e) {
    put(kind, id, before);
    emit();
    throw e;
  } finally {
    if (pending.get(key) === on) pending.delete(key);
  }
  refreshData();
}

// -- 다른 화면에서 바뀌었을 때 ----------------------------------------------

let lastGeneration = -1;

/** /api/events가 알려 준 좋아요·플레이리스트 세대. 바뀌었으면 다시 받는다. */
export function noteCollections(generation: number | undefined) {
  if (generation === undefined) return;
  if (lastGeneration !== -1 && generation !== lastGeneration) {
    void refreshFavorites();
    refreshData();
  }
  lastGeneration = generation;
}

// -- 플레이리스트 -----------------------------------------------------------

function send(method: string, body?: unknown): RequestInit {
  return body === undefined
    ? { method }
    : { method, headers: { 'content-type': 'application/json' }, body: JSON.stringify(body) };
}

async function changed<T>(p: Promise<T>): Promise<T> {
  const v = await p;
  refreshData();
  return v;
}

export const playlists = {
  create: (name: string, tracks: string[] = []) => changed(fetchJson<PlaylistSummary>('/api/playlists', send('POST', { name, tracks }))),
  update: (id: string, patch: { name?: string; tracks?: string[] }) =>
    changed(fetchJson<PlaylistSummary>(`/api/playlists/${encodeURIComponent(id)}`, send('PATCH', patch))),
  add: (id: string, ids: string[]) => changed(fetchJson<PlaylistSummary>(`/api/playlists/${encodeURIComponent(id)}/tracks`, send('POST', { ids }))),
  remove: (id: string) => changed(fetchJson<{ ok: boolean }>(`/api/playlists/${encodeURIComponent(id)}`, send('DELETE'))),
};

/** 목록의 한 칸을 옮긴다(새 배열). */
export function moveItem<T>(list: readonly T[], from: number, to: number): T[] {
  const out = list.slice();
  if (from < 0 || from >= out.length || to < 0 || to >= out.length || from === to) return out;
  const [it] = out.splice(from, 1);
  out.splice(to, 0, it!);
  return out;
}

// -- "플레이리스트에 추가" 창 -------------------------------------------------

export interface PickerRequest {
  ids: string[];
  /** 창 위에 적을 것: 곡 이름이나 "대기열 12곡" */
  label: string;
  /** 새로 만들 때 미리 채워 둘 이름 */
  suggest?: string;
}

let picker: PickerRequest | null = null;
const pickerListeners = new Set<() => void>();

function setPicker(p: PickerRequest | null) {
  picker = p;
  for (const l of pickerListeners) l();
}

export function openPlaylistPicker(req: PickerRequest) {
  if (req.ids.length) setPicker(req);
}

export function closePlaylistPicker() {
  setPicker(null);
}

export function usePlaylistPicker(): PickerRequest | null {
  return useSyncExternalStore(
    (l) => {
      pickerListeners.add(l);
      return () => {
        pickerListeners.delete(l);
      };
    },
    () => picker,
    () => null,
  );
}

/** 시험용: 모듈 상태를 처음으로 */
export function resetCollectionsForTest() {
  liked.tracks = new Set();
  liked.albums = new Set();
  pending.clear();
  loaded = false;
  loading = null;
  again = false;
  lastGeneration = -1;
  picker = null;
}
