import { clipboard, preferencesGet, preferencesSet } from './platform';
import type { GameState, GameSummary } from './game';

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
}

export const DEFAULT_SETTINGS: AppSettings = {
  myName: '나',
  lastKind: 'four',
  lastTargets: { white: 20, yellow: 20 },
  lastCushion: 0,
  lastEqualizer: false,
  lastFoul: false,
  lastVenue: '',
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

export async function loadHistory(): Promise<GameSummary[]> {
  const list = await readJson<GameSummary[]>(HISTORY, []);
  return Array.isArray(list) ? list : [];
}

/** 끝난 게임을 기록에 넣는다. 같은 id가 이미 있으면 덮어쓴다 — 되돌리고 다시 끝낸 경우다. */
export async function recordGame(summary: GameSummary): Promise<GameSummary[]> {
  const list = (await loadHistory()).filter((entry) => entry.id !== summary.id);
  const next = [summary, ...list].slice(0, HISTORY_LIMIT);
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
  const next = list.map((entry) => (entry.id === summary.id ? summary : entry));
  await writeJson(HISTORY, next);
  return next;
}

/** 기기에 있는 기록을 전부 지운다. 되돌릴 수 없으므로 부르는 쪽에서 한 번 물어본다. */
export async function clearHistory(): Promise<GameSummary[]> {
  await writeJson(HISTORY, []);
  return [];
}

export async function removeGame(id: string): Promise<GameSummary[]> {
  const next = (await loadHistory()).filter((entry) => entry.id !== id);
  await writeJson(HISTORY, next);
  return next;
}

/* 설정 -------------------------------------------------------------- */

export const loadSettings = () => readJson<AppSettings>(SETTINGS, DEFAULT_SETTINGS);
export const saveSettings = (settings: AppSettings) => writeJson(SETTINGS, settings);

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
