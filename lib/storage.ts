import { clipboard, preferencesGet, preferencesSet } from './platform';
import { SCHEMA, migrateAll, type GameState, type GameSummary } from './game';

/**
 * 저장. 로컬이 먼저고 클라우드는 나중이다.
 *
 * 당구장은 지하가 많고 와이파이는 대체로 없다. 그래서 진행 중인 게임과 최근 기록은
 * 기기에 쓰고, 로그인이 되어 있으면 그 다음에 Firestore로 올린다. 순서가 반대였다면
 * 네트워크가 없는 순간 점수판이 멈춘다.
 */

const CURRENT = 'dangu.current';
const HISTORY = 'dangu.history';
const SETTINGS = 'dangu.settings';
const TRASH = 'dangu.trash';

/** 최근 기록은 기기에 이만큼만 둔다. 그 이상은 로그인한 사람의 클라우드에 있다. */
const HISTORY_LIMIT = 50;

/**
 * 기록을 어디에 둘지.
 *
 * 기본은 `local` — 계정 없이 켜서 바로 치는 게 이 앱의 기본 사용법이고, 아무것도
 * 고르지 않은 사람의 데이터가 조용히 클라우드로 나가지 않아야 한다. `cloud`는 로그인한
 * 사람이 명시적으로 고르는 값이며, 그때도 기기 저장은 계속된다: 클라우드는 사본이지
 * 원본이 아니다. 당구장에 네트워크가 없어도 게임이 끝까지 돌아가야 하기 때문이다.
 */
export type SyncMode = 'local' | 'cloud';

export interface AppSettings {
  /** 로비에 미리 채워질 내 이름. 플레이어 1은 거의 늘 같은 사람이다. */
  myName: string;
  /** 마지막으로 고른 종목과 핸디 — 다음 게임도 대개 같다. */
  lastKind: string;
  lastTargets: { white: number; yellow: number };
  /** 4구에서 마지막 몇 점을 쿠션으로 칠지. 다음 게임에도 대개 같은 값을 쓴다. */
  lastCushion: number;
  /** 후구를 쓸지. 같이 치는 사람들끼리는 대개 늘 같은 규칙으로 친다. */
  lastEqualizer: boolean;
  /** 뒷빡을 쓸지. 후구와 같은 이유로 기억해 둔다. */
  lastFoul: boolean;
  /**
   * 마지막으로 친 당구장.
   *
   * 사람은 대개 같은 집에 간다. 그래서 다음 판의 기본값이 되고, 바꾸는 날에만 손이 간다.
   */
  lastVenue: string;
  /**
   * 내가 다니는 당구장들.
   *
   * 기록에서 되짚을 수도 있지만 그건 "친 적 있는 집"이고, 이건 "다니는 집"이다. 둘은
   * 다르다 — 새로 생긴 집은 아직 기록이 없어도 오늘 갈 수 있고, 한 번 가 보고 만 집은
   * 기록에는 남아도 목록에 세울 이유가 없다. 화면에는 둘을 합쳐 보여 준다.
   */
  venues: string[];
  haptics: boolean;
  keepAwake: boolean;
  /**
   * 점수를 올릴 때 남은 점수를 소리 내어 읽을지.
   *
   * 큐를 들고 있는 사람은 화면을 보고 있지 않다. 기본은 켬 — 이 앱을 켜 두는 자리가
   * 대개 테이블 옆이고, 거기서 제일 자주 하는 일이 "몇 개 남았지?"를 묻는 것이다.
   */
  voice: boolean;
  /**
   * 한 차례에 주는 시간(초). 0이면 안 쓴다.
   *
   * 3쿠션의 공식 샷 클락이 40초라 그걸 기본으로 둔다. 동호회에서는 더 넉넉하게 잡는
   * 일이 흔하므로 고를 수 있게 했다.
   */
  turnSeconds: number;
  sync: SyncMode;
  /**
   * 저장 방식을 사람이 직접 골랐는지.
   *
   * 기본값 `local`에는 두 가지 뜻이 겹쳐 있었다 — "아직 아무것도 고르지 않았다"와
   * "이 기기에만 두기로 정했다". 로그인한 사람의 기록을 자동으로 계정에 옮기려면 그
   * 둘을 갈라야 한다. 앞엣것은 옮겨도 되는 상태이고, 뒤엣것은 사람이 내린 결정이라
   * 앱이 뒤집으면 안 된다.
   */
  syncPinned?: boolean;
  /**
   * 이 설정이 마지막으로 바뀐 시각.
   *
   * 기기 두 대가 같은 계정으로 설정을 고쳤을 때 어느 쪽을 남길지 정하는 값이다. 기록과
   * 같은 규칙을 쓴다 — 나중에 고친 것이 이긴다.
   */
  updatedAt?: number;
}

/**
 * 계정을 따라다니는 설정들.
 *
 * 여기 없는 값은 기기의 것이라 옮기지 않는다. 저장 방식(`sync`, `syncPinned`)은 기기마다
 * 다를 수 있고 — 태블릿은 로그인해 두고 남의 폰에서는 안 할 수 있다 — 진동과 화면 켜
 * 두기는 그 기기가 무엇을 할 수 있는지에 대한 값이다. 브라우저에는 진동이 없다.
 *
 * 나머지는 사람의 것이다. 내 이름, 다니는 당구장, 늘 치는 종목과 핸디는 폰을 바꿔도
 * 그대로여야 하는 값이고, 그게 이 목록이 있는 이유다.
 */
export const SHARED_SETTINGS = [
  'myName',
  'lastKind',
  'lastTargets',
  'lastCushion',
  'lastEqualizer',
  'lastFoul',
  'lastVenue',
  'venues',
  'voice',
  'turnSeconds',
] as const satisfies readonly (keyof AppSettings)[];

export type SharedSettings = Pick<AppSettings, (typeof SHARED_SETTINGS)[number]> & {
  updatedAt: number;
};

export const DEFAULT_SETTINGS: AppSettings = {
  myName: '나',
  lastKind: 'four',
  lastTargets: { white: 20, yellow: 20 },
  lastCushion: 0,
  lastEqualizer: false,
  lastFoul: false,
  lastVenue: '',
  venues: [],
  haptics: true,
  keepAwake: true,
  voice: true,
  turnSeconds: 0,
  sync: 'local',
};

/** 클라우드 저장이 켜져 있는지. 로그인 여부는 부르는 쪽이 따로 본다. */
export const cloudChosen = async () => (await loadSettings()).sync === 'cloud';

/**
 * 저장된 JSON 하나를 읽는다.
 *
 * 기본값 위에 얹는 병합은 *객체에만* 한다. 배열에 `{ ...fallback, ...parsed }`를 하면
 * `["a","b"]`가 `{0:"a",1:"b"}`가 되어, 기록은 멀쩡히 저장되어 있는데 읽는 쪽에서
 * 배열이 아니라는 이유로 통째로 버려진다 — 저장이 안 되는 것처럼 보이는 버그가
 * 정확히 이것이었다. 설정처럼 나중에 키가 늘어나는 객체에만 병합이 의미가 있다.
 */
async function readJson<T>(key: string, fallback: T): Promise<T> {
  const raw = await preferencesGet(key);
  if (!raw) return fallback;
  try {
    const parsed = JSON.parse(raw) as T;
    if (parsed === null || typeof parsed !== 'object' || Array.isArray(parsed)) return parsed;
    if (fallback === null || typeof fallback !== 'object' || Array.isArray(fallback)) return parsed;
    return { ...fallback, ...parsed };
  } catch {
    // 깨진 값 하나가 앱을 못 켜게 만드는 것보다, 조용히 기본값으로 시작하는 편이 낫다.
    return fallback;
  }
}

const writeJson = (key: string, value: unknown) => preferencesSet(key, JSON.stringify(value));

/* 진행 중인 게임 --------------------------------------------------- */

export const loadCurrentGame = () => readJson<GameState | null>(CURRENT, null);
export const saveCurrentGame = (state: GameState) => writeJson(CURRENT, state);
export const clearCurrentGame = () => preferencesSet(CURRENT, '');

/* 기록 -------------------------------------------------------------- */

/**
 * 기기에 있는 기록.
 *
 * 읽으면서 옛 모양을 지금 모양으로 옮긴다. 옮길 것이 있었을 때만 다시 쓴다 — 앱을 켤
 * 때마다 오십 줄을 다시 저장하면 그건 읽기가 아니라 쓰기다.
 */
export async function loadHistory(): Promise<GameSummary[]> {
  const list = await readJson<GameSummary[]>(HISTORY, []);
  if (!Array.isArray(list)) return [];
  const { games, changed } = migrateAll(list);
  if (changed.length) await writeJson(HISTORY, games);
  return games;
}

/** 끝난 게임을 기록에 넣는다. 같은 id가 이미 있으면 덮어쓴다 — 되돌리고 다시 끝낸 경우다. */
export async function recordGame(summary: GameSummary): Promise<GameSummary[]> {
  const list = (await loadHistory()).filter((entry) => entry.id !== summary.id);
  const stamped = { ...summary, schema: SCHEMA, updatedAt: Date.now() };
  const next = [stamped, ...list].slice(0, HISTORY_LIMIT);
  await writeJson(HISTORY, next);
  return next;
}

/* 지운 기록 --------------------------------------------------------- */

/**
 * 지운 판의 이름표 — id와 지운 시각.
 *
 * 기기가 하나뿐이라면 지우기는 그냥 목록에서 빼는 일이다. 둘이 되는 순간 그렇지 않다:
 * 폰에서 지운 판이 태블릿에는 아직 있고, 태블릿은 "클라우드에 없는 판"을 올리는 것이
 * 자기 할 일이므로 그걸 다시 올린다. 지운 것이 몇 초 뒤에 되살아난다.
 *
 * 그래서 지운 사실도 데이터다. "없다"와 "지웠다"는 다르고, 앞엣것만으로는 두 기기가
 * 서로를 되돌린다.
 */
export type Trash = Record<string, number>;

/**
 * 이름표를 이만큼 들고 있는다(180일).
 *
 * 영원히 둘 이유는 없지만 짧게 두면 안 된다 — 반년 만에 켠 태블릿이 그 사이에 지운
 * 판들을 도로 올리는 것이 이 값을 짧게 잡았을 때 생기는 일이다.
 */
const TRASH_KEEP = 180 * 24 * 60 * 60 * 1000;

export async function loadTrash(): Promise<Trash> {
  const trash = await readJson<Trash>(TRASH, {});
  if (!trash || typeof trash !== 'object' || Array.isArray(trash)) return {};
  const cutoff = Date.now() - TRASH_KEEP;
  const kept: Trash = {};
  let dropped = false;
  for (const [id, at] of Object.entries(trash)) {
    if (typeof at === 'number' && at > cutoff) kept[id] = at;
    else dropped = true;
  }
  if (dropped) await writeJson(TRASH, kept);
  return kept;
}

/**
 * 지운 판들을 이름표에 적는다. 이미 적힌 것은 나중 시각으로 갱신한다.
 *
 * 처음 지운 시각을 남기면 안 된다. 지웠다가 되살아난 판 — 점수판에서 끝낸 판을 되돌려
 * 다시 친 경우가 그렇다 — 을 나중에 또 지우면, 옛 시각은 그 판의 `updatedAt`보다 앞서
 * 있으므로 이름표가 아무 일도 하지 못한다. 지우기가 조용히 안 먹는다.
 */
export async function markDeleted(ids: string[], at = Date.now()): Promise<Trash> {
  if (!ids.length) return loadTrash();
  const trash = await loadTrash();
  for (const id of ids) trash[id] = Math.max(trash[id] ?? 0, at);
  await writeJson(TRASH, trash);
  return trash;
}

/**
 * 다른 기기가 지운 것을 받아 적고, 여기 남아 있으면 함께 지운다.
 *
 * 목록이 실제로 달라졌을 때만 기록을 다시 쓴다 — 스냅숏은 붙어 있는 동안 여러 번
 * 오고, 그때마다 오십 줄을 저장할 이유는 없다.
 */
export async function applyTrash(incoming: Trash): Promise<GameSummary[] | null> {
  const entries = Object.entries(incoming).filter(([, at]) => typeof at === 'number');
  if (!entries.length) return null;

  const trash = await loadTrash();
  let noted = false;
  for (const [id, at] of entries) {
    if ((trash[id] ?? 0) >= at) continue;
    trash[id] = at;
    noted = true;
  }
  if (noted) await writeJson(TRASH, trash);

  const list = await loadHistory();
  const next = list.filter((game) => !buried(trash, game));
  if (next.length === list.length) return null;
  await writeJson(HISTORY, next);
  return next;
}

/**
 * 이 판이 지워진 것인지.
 *
 * 지운 뒤에 다른 기기에서 고쳤다면 되살아난다 — 기록끼리 부딪혔을 때 쓰는 규칙과 같다.
 * 나중에 한 일이 그 사람의 마지막 뜻이라고 보는 것이 제일 덜 틀린다.
 */
const buried = (trash: Trash, game: GameSummary) => (trash[game.id] ?? 0) >= (game.updatedAt ?? 0);

/**
 * 클라우드에서 온 기록을 기기 목록에 얹는다.
 *
 * 같은 id가 이미 있으면 나중에 고쳐진 쪽을 남긴다. 자리는 원래 있던 자리 그대로다 —
 * 목록의 순서는 친 날짜의 것이고, 어느 기기에서 언제 내려왔는지와는 상관이 없다.
 * 새로 온 것은 날짜 순서에 맞춰 끼운다.
 *
 * 지운 판은 다시 들이지 않는다. 다른 기기가 아직 그 판을 들고 있어도 마찬가지다 —
 * 그쪽도 곧 이름표를 받고 지운다.
 */
export async function mergeGames(incoming: GameSummary[]): Promise<GameSummary[]> {
  const list = await loadHistory();
  const trash = await loadTrash();
  const byId = new Map(list.map((entry) => [entry.id, entry]));
  let touched = false;

  for (const game of incoming) {
    if (buried(trash, game)) continue;
    const here = byId.get(game.id);
    if (!here) {
      byId.set(game.id, game);
      touched = true;
      continue;
    }
    if ((game.updatedAt ?? 0) > (here.updatedAt ?? 0)) {
      byId.set(game.id, game);
      touched = true;
    }
  }

  if (!touched) return list;
  const next = [...byId.values()]
    .sort((a, b) => b.startedAt - a.startedAt)
    .slice(0, HISTORY_LIMIT);
  await writeJson(HISTORY, next);
  return next;
}

/**
 * 이미 저장된 기록 하나를 고친다.
 *
 * `recordGame`과 달리 자리를 옮기지 않는다 — 8월 3일에 친 게임의 점수를 고쳤다고 해서
 * 그게 목록 맨 위로 올라오면, 고친 사람이 자기가 무엇을 건드렸는지 잃어버린다. 목록의
 * 순서는 친 날짜의 것이지 고친 날짜의 것이 아니다.
 */
export async function updateGame(summary: GameSummary): Promise<GameSummary[]> {
  const list = await loadHistory();
  const stamped = { ...summary, schema: SCHEMA, updatedAt: Date.now() };
  const next = list.map((entry) => (entry.id === summary.id ? stamped : entry));
  await writeJson(HISTORY, next);
  return next;
}

/** 기기에 있는 기록을 전부 지운다. 되돌릴 수 없으므로 부르는 쪽에서 한 번 물어본다. */
export async function clearHistory(): Promise<GameSummary[]> {
  const list = await loadHistory();
  await markDeleted(list.map((game) => game.id));
  await writeJson(HISTORY, []);
  return [];
}

export async function removeGame(id: string): Promise<GameSummary[]> {
  await markDeleted([id]);
  const next = (await loadHistory()).filter((entry) => entry.id !== id);
  await writeJson(HISTORY, next);
  return next;
}

/* 설정 -------------------------------------------------------------- */

export const loadSettings = () => readJson<AppSettings>(SETTINGS, DEFAULT_SETTINGS);

/**
 * 설정이 바뀐 것을 지켜본다.
 *
 * 설정을 고치는 자리가 한 곳이 아니게 되면서 필요해졌다 — 상단 줄에서 당구장을 고르면
 * 로비의 장소 칸도 그걸 따라야 하는데, 둘은 서로 다른 아일랜드라 상태를 나눠 가질 수
 * 없다. 저장이 일어난 자리에서 알려 주는 편이 각자 다시 읽는 것보다 정확하다.
 */
const watchers = new Set<(settings: AppSettings) => void>();

export function watchSettings(listener: (settings: AppSettings) => void): () => void {
  watchers.add(listener);
  return () => {
    watchers.delete(listener);
  };
}

/**
 * 설정을 저장한다.
 *
 * 저장할 때마다 시각을 찍는다. 그 값이 기기 두 대가 같은 설정을 고쳤을 때 어느 쪽을
 * 남길지 정하는 근거다. `stamp: false`는 클라우드에서 받은 값을 그대로 놓을 때만 쓴다 —
 * 받은 것에 지금 시각을 찍으면 그게 다시 "더 새것"이 되어 상대 기기로 밀려가고, 두
 * 기기가 같은 값을 서로에게 영원히 밀어 준다.
 */
export async function saveSettings(
  settings: AppSettings,
  options: { stamp?: boolean } = {}
): Promise<void> {
  const next = options.stamp === false ? settings : { ...settings, updatedAt: Date.now() };
  await writeJson(SETTINGS, next);
  for (const listener of watchers) listener(next);
}

/** 계정을 따라다니는 값들만 뽑아낸다. 올릴 때 쓰는 모양이다. */
export function sharedSettings(settings: AppSettings): SharedSettings {
  const shared = { updatedAt: settings.updatedAt ?? 0 } as SharedSettings;
  for (const key of SHARED_SETTINGS) {
    const value = settings[key];
    // 없는 값은 키를 만들지 않는다 — Firestore는 `undefined`가 든 문서를 거절한다.
    if (value !== undefined) (shared as Record<string, unknown>)[key] = value;
  }
  return shared;
}

/**
 * 클라우드에서 온 설정을 얹는다. 실제로 바뀌었으면 새 설정을, 아니면 `null`을 돌려준다.
 *
 * 더 오래된 것은 버린다. 기기의 값이 더 새것이라면 그건 이 기기가 방금 고친 것이고,
 * 곧 반대 방향으로 올라간다.
 */
export async function applyRemoteSettings(
  remote: Partial<AppSettings> & { updatedAt?: number }
): Promise<AppSettings | null> {
  const current = await loadSettings();
  const at = remote.updatedAt ?? 0;
  if (at <= (current.updatedAt ?? 0)) return null;

  const next: AppSettings = { ...current, updatedAt: at };
  let changed = false;
  for (const key of SHARED_SETTINGS) {
    const value = remote[key];
    if (value === undefined) continue;
    if (JSON.stringify(value) === JSON.stringify(current[key])) continue;
    Object.assign(next, { [key]: value });
    changed = true;
  }

  // 값은 같고 시각만 새것인 경우 — 다른 기기가 상관없는 것을 고쳤다. 시각은 맞춰 두되
  // 화면을 다시 그리게 하지는 않는다.
  await writeJson(SETTINGS, next);
  if (!changed) return null;
  for (const listener of watchers) listener(next);
  return next;
}

/**
 * 이 기기가 계정에 처음 붙는 순간의 설정 합치기.
 *
 * 평소의 규칙은 "나중에 고친 쪽이 이긴다"이고 그건 옳다 — 같은 값을 두 곳에서 고쳤으면
 * 사람이 마지막으로 한 말이 그 사람의 뜻이다. 그런데 *처음* 붙는 순간만은 다르다.
 * 그때 마주 서는 두 벌은 "같은 값의 두 판본"이 아니라 서로를 모르고 각자 쌓인 두
 * 살림이고, 시각으로 하나를 고르면 나머지 하나가 통째로 사라진다. 폰에 반년 적어 둔
 * 당구장 목록이 태블릿에 로그인하는 순간 없어지는 것이 그 일이다.
 *
 * 그래서 여기서만 합친다. 목록은 합집합으로 — 두 곳에 적어 둔 집 중 하나를 버릴
 * 이유가 없다. 나머지 값은 하나만 남을 수밖에 없으므로 나중 것을 쓴다. 합친 뒤로는
 * 두 기기가 같은 것을 보고 있으니 평소 규칙으로 돌아간다.
 */
export async function joinRemoteSettings(
  remote: (Partial<AppSettings> & { updatedAt?: number }) | null
): Promise<{ settings: AppSettings; changed: boolean }> {
  const local = await loadSettings();
  if (!remote) return { settings: local, changed: true };

  const newer = (remote.updatedAt ?? 0) > (local.updatedAt ?? 0) ? remote : local;
  const older = newer === remote ? local : remote;
  const next: AppSettings = { ...local };
  for (const key of SHARED_SETTINGS) {
    const value = newer[key];
    if (value !== undefined) Object.assign(next, { [key]: value });
  }

  /*
   * 다니는 당구장은 목록이라 합칠 수 있다. 순서는 나중에 고친 쪽을 앞에 둔다.
   *
   * "이 기기 것을 앞에" 두면 안 된다. 그러면 합친 결과가 어느 기기에서 합쳤느냐에 따라
   * 달라지고, 두 기기는 서로의 목록을 영영 "내 것과 다른 값"으로 본다 — 붙을 때마다
   * 순서를 뒤집어 상대에게 밀어 주는 일이 끝나지 않는다. 합치기는 어디서 하든 같은
   * 답이 나와야 하고, 그 기준으로 쓸 수 있는 것은 두 기기가 똑같이 보는 값뿐이다.
   */
  const venues = [...(newer.venues ?? [])];
  for (const name of older.venues ?? []) {
    if (!venues.includes(name)) venues.push(name);
  }
  next.venues = venues;

  // 합친 결과가 클라우드에 있는 것과 다르면 그건 이 기기가 만든 새 값이고, 다른 기기도
  // 그것을 받아야 한다. 같으면 시각만 맞춰 두고 아무것도 올리지 않는다.
  const changed = SHARED_SETTINGS.some(
    (key) => JSON.stringify(next[key]) !== JSON.stringify(remote[key])
  );
  if (changed) return { settings: next, changed: true };

  /*
   * 합칠 것이 없었다. 시각만 맞춰 두고 아무에게도 알리지 않는다.
   *
   * 알리지 않는 것이 중요하다. 지켜보는 자리 중 하나가 "설정이 바뀌었으니 올린다"이고,
   * 여기서 그걸 깨우면 방금 받은 것과 똑같은 문서를 도로 올린다 — 그 쓰기는 다른 기기에
   * 스냅숏으로 닿고, 그쪽도 같은 이유로 되민다. 값이 바뀌지 않은 저장은 사건이 아니다.
   */
  next.updatedAt = remote.updatedAt ?? next.updatedAt;
  await writeJson(SETTINGS, next);
  return { settings: next, changed: false };
}

/* 내보내기 ---------------------------------------------------------- */

/**
 * 기록을 클립보드로. 계정 없이 쓰던 사람이 폰을 바꿀 때 남는 유일한 길이라,
 * 로그인 없이도 데이터를 꺼낼 수 있어야 한다.
 */
export async function copyHistory(): Promise<{ supported: boolean; reason?: string }> {
  const list = await loadHistory();
  const text = list
    .map((entry) => {
      const date = new Date(entry.startedAt).toLocaleString('ko-KR');
      const white = `${entry.players.white.name} ${entry.players.white.score}/${entry.players.white.target}`;
      const yellow = `${entry.players.yellow.name} ${entry.players.yellow.score}/${entry.players.yellow.target}`;
      return `${date}\t${entry.kind}\t${white}\t${yellow}\t${entry.inning}이닝`;
    })
    .join('\n');

  const result = await clipboard.writeText(text || '기록 없음');
  return result.supported ? { supported: true } : { supported: false, reason: result.reason };
}
