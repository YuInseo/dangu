import { preferencesGet, preferencesSet } from './platform';

/**
 * 자주 적는 말 — 쪽지 칸마다 몇 번 썼는지.
 *
 * 한 역에서 받는 쪽지는 행선지도, 만나는 자리도 늘 비슷하다. 그걸 매번 치게 하지 않고
 * 많이 쓴 순서로 칸 아래에 띄워, 누르면 바로 들어가게 한다. 내 손에 익은 말이므로 기기에 둔다.
 */

export type RecentField = 'station' | 'meet' | 'memo';

interface Entry {
  value: string;
  count: number;
  /** 마지막으로 쓴 때. 횟수가 같으면 최근 것이 앞. */
  at: number;
}

type Store = Partial<Record<RecentField, Entry[]>>;

const KEY = 'donghaeng.recent';
// 칸마다 이만큼만 기억한다. 넘치면 덜 쓴 것부터 잊는다.
const KEEP = 30;

async function load(): Promise<Store> {
  try {
    const text = await preferencesGet(KEY);
    const value = text ? JSON.parse(text) : null;
    return value && typeof value === 'object' ? value : {};
  } catch {
    return {};
  }
}

const rank = (a: Entry, b: Entry) => b.count - a.count || b.at - a.at;

/** 많이 쓴 순서로 `limit`개. */
export async function recentValues(limit = 6): Promise<Record<RecentField, string[]>> {
  const store = await load();
  const pick = (field: RecentField) => [...(store[field] ?? [])].sort(rank).slice(0, limit).map((entry) => entry.value);
  return { station: pick('station'), meet: pick('meet'), memo: pick('memo') };
}

/** 저장한 쪽지의 칸들을 센다. 빈 칸은 세지 않는다. */
export async function remember(values: Partial<Record<RecentField, string>>): Promise<void> {
  const store = await load();
  const now = Date.now();
  for (const [field, raw] of Object.entries(values) as [RecentField, string | undefined][]) {
    const value = raw?.trim();
    if (!value) continue;
    const list = store[field] ?? [];
    const found = list.find((entry) => entry.value === value);
    if (found) {
      found.count += 1;
      found.at = now;
    } else {
      list.push({ value, count: 1, at: now });
    }
    store[field] = list.sort(rank).slice(0, KEEP);
  }
  await preferencesSet(KEY, JSON.stringify(store));
}

/** 잘못 적어 남은 말을 지운다. */
export async function forget(field: RecentField, value: string): Promise<void> {
  const store = await load();
  store[field] = (store[field] ?? []).filter((entry) => entry.value !== value);
  await preferencesSet(KEY, JSON.stringify(store));
}
