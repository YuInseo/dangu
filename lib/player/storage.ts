/** localStorage는 사생활 보호 창에서 던지기도 한다. 실패해도 재생은 계속된다. */
export function readJSON<T>(key: string): T | null {
  try {
    const raw = localStorage.getItem(key);
    return raw ? (JSON.parse(raw) as T) : null;
  } catch {
    return null;
  }
}

export function writeJSON(key: string, value: unknown) {
  try {
    localStorage.setItem(key, JSON.stringify(value));
  } catch {
    // 꽉 찼거나 막혔다
  }
}

export interface RecentEntry {
  kind: 'album' | 'live';
  id: string;
  title: string;
  artist: string;
  cover?: { sm: string; lg: string; color?: string };
  seed: string;
  at: number;
}

const RECENT = 'sori.recent';

export function readRecent(): RecentEntry[] {
  return readJSON<RecentEntry[]>(RECENT) ?? [];
}

export function pushRecent(e: Omit<RecentEntry, 'at'>) {
  const list = readRecent().filter((r) => !(r.kind === e.kind && r.id === e.id));
  list.unshift({ ...e, at: Date.now() });
  writeJSON(RECENT, list.slice(0, 24));
  window.dispatchEvent(new Event('sori:recent'));
}
