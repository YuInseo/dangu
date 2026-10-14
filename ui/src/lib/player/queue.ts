/**
 * 대기열 규칙. React를 모르는 순수 함수라 따로 시험할 수 있다.
 */
import type { PlayableTrack, QueueItem, RepeatMode } from './types';

export interface QueueState {
  items: QueueItem[];
  index: number;
  shuffle: boolean;
  repeat: RepeatMode;
  /** 셔플을 켜기 전 순서. 끄면 이 순서로 돌아간다. */
  unshuffled: QueueItem[] | null;
  /** 같은 곡이라도 다시 걸어야 할 때 올린다. */
  token: number;
}

export type QueueAction =
  | { type: 'replace'; items: QueueItem[]; index: number; shuffle?: boolean }
  | { type: 'insertNext'; items: QueueItem[] }
  | { type: 'append'; items: QueueItem[] }
  | { type: 'jump'; index: number }
  | { type: 'remove'; index: number }
  | { type: 'move'; from: number; to: number }
  | { type: 'clearUpcoming' }
  | { type: 'clearAll' }
  | { type: 'toggleShuffle' }
  | { type: 'cycleRepeat' }
  | { type: 'restore'; state: QueueState };

export const initialQueue: QueueState = { items: [], index: 0, shuffle: false, repeat: 'off', unshuffled: null, token: 0 };

let seq = 0;
export function wrap(tracks: PlayableTrack[]): QueueItem[] {
  return tracks.map((track) => ({
    uid: `${Date.now().toString(36)}${(seq++).toString(36)}${Math.random().toString(36).slice(2, 6)}`,
    track,
  }));
}

function shuffled<T>(xs: T[]): T[] {
  const a = [...xs];
  for (let i = a.length - 1; i > 0; i--) {
    const j = Math.floor(Math.random() * (i + 1));
    [a[i], a[j]] = [a[j]!, a[i]!];
  }
  return a;
}

/** 곡이 끝났을 때 갈 곳. 한 곡 반복은 여기서 다루지 않는다. */
export function nextIndex(s: QueueState): number | null {
  if (!s.items.length) return null;
  if (s.index + 1 < s.items.length) return s.index + 1;
  return s.repeat === 'off' ? null : 0;
}

export function queueReducer(s: QueueState, a: QueueAction): QueueState {
  switch (a.type) {
    case 'replace': {
      const shuffle = a.shuffle ?? s.shuffle;
      let items = a.items;
      let index = Math.max(0, Math.min(a.index, items.length - 1));
      let unshuffled: QueueItem[] | null = null;
      if (shuffle && items.length > 1) {
        unshuffled = items;
        if (a.shuffle === true) {
          // "셔플" 버튼: 첫 곡부터 무작위.
          items = shuffled(items);
        } else {
          // 셔플이 켜진 채로 곡을 고르면 그 곡부터, 나머지는 섞어서.
          items = [items[index]!, ...shuffled(items.filter((_, i) => i !== index))];
        }
        index = 0;
      }
      return { ...s, items, index, shuffle, unshuffled, token: s.token + 1 };
    }
    case 'insertNext': {
      if (!s.items.length) return { ...s, items: a.items, index: 0, unshuffled: null, token: s.token + 1 };
      const items = [...s.items.slice(0, s.index + 1), ...a.items, ...s.items.slice(s.index + 1)];
      let unshuffled = s.unshuffled;
      if (unshuffled) {
        const at = unshuffled.findIndex((i) => i.uid === s.items[s.index]!.uid);
        unshuffled = [...unshuffled.slice(0, at + 1), ...a.items, ...unshuffled.slice(at + 1)];
      }
      return { ...s, items, unshuffled };
    }
    case 'append': {
      if (!s.items.length) return { ...s, items: a.items, index: 0, unshuffled: null, token: s.token + 1 };
      return { ...s, items: [...s.items, ...a.items], unshuffled: s.unshuffled ? [...s.unshuffled, ...a.items] : null };
    }
    case 'jump':
      return { ...s, index: Math.max(0, Math.min(a.index, s.items.length - 1)), token: s.token + 1 };
    case 'remove': {
      const gone = s.items[a.index];
      if (!gone) return s;
      const items = s.items.filter((_, i) => i !== a.index);
      let index = s.index;
      if (a.index < s.index) index--;
      else if (a.index === s.index) index = Math.min(index, items.length - 1);
      return {
        ...s,
        items,
        index: Math.max(0, index),
        unshuffled: s.unshuffled?.filter((i) => i.uid !== gone.uid) ?? null,
        token: a.index === s.index ? s.token + 1 : s.token,
      };
    }
    case 'move': {
      if (a.from === a.to || !s.items[a.from]) return s;
      const cur = s.items[s.index]?.uid;
      const items = [...s.items];
      const [m] = items.splice(a.from, 1);
      items.splice(a.to, 0, m!);
      return { ...s, items, index: items.findIndex((i) => i.uid === cur) };
    }
    case 'clearUpcoming': {
      const items = s.items.slice(0, s.index + 1);
      return { ...s, items, unshuffled: s.unshuffled ? items : null };
    }
    case 'clearAll':
      return { ...initialQueue, shuffle: s.shuffle, repeat: s.repeat, token: s.token + 1 };
    case 'toggleShuffle': {
      if (!s.shuffle) {
        return {
          ...s,
          shuffle: true,
          unshuffled: s.items,
          items: [...s.items.slice(0, s.index + 1), ...shuffled(s.items.slice(s.index + 1))],
        };
      }
      const cur = s.items[s.index]?.uid;
      let items = s.items;
      if (s.unshuffled) {
        const present = new Set(s.items.map((i) => i.uid));
        const base = s.unshuffled.filter((i) => present.has(i.uid));
        const inBase = new Set(base.map((i) => i.uid));
        items = [...base, ...s.items.filter((i) => !inBase.has(i.uid))];
      }
      return {
        ...s,
        shuffle: false,
        unshuffled: null,
        items,
        index: Math.max(
          0,
          items.findIndex((i) => i.uid === cur),
        ),
      };
    }
    case 'cycleRepeat':
      return { ...s, repeat: s.repeat === 'off' ? 'all' : s.repeat === 'all' ? 'one' : 'off' };
    case 'restore':
      return a.state;
  }
}
