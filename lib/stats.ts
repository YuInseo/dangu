import { GAME_KINDS, kindInfo, other, sides, type GameKind, type GameSummary, type Side } from './game';

/**
 * 통계. 기록 배열 하나를 받아 숫자만 낸다 — 저장소도 화면도 모른다.
 *
 * "오늘 몇 게임"과 "이번 달 몇 게임"은 자정과 월초를 어떻게 보느냐의 문제다. 여기서는
 * 전부 기기의 지역 시간으로 자른다: 새벽 두 시에 친 게임은 그 사람에게 어제 친 게임이
 * 아니라 오늘 친 게임이고, 날짜 경계를 UTC로 잡으면 그게 어긋난다.
 */

export interface Tally {
  games: number;
  wins: number;
  losses: number;
  /** 승률 0–1. 게임이 없으면 0이다 — 0/0을 100%로 부르지 않는다. */
  rate: number;
  /** 총 친 시간(ms). */
  elapsedMs: number;
  points: number;
  innings: number;
}

export interface OpponentTally extends Tally {
  name: string;
  lastPlayedAt: number;
}

export interface KindTally extends Tally {
  kind: GameKind;
  label: string;
}

export interface Stats {
  today: Tally;
  month: Tally;
  all: Tally;
  /** 많이 친 순서. 같은 이름은 한 사람으로 본다. */
  opponents: OpponentTally[];
  kinds: KindTally[];
  /** 요즘 흐름 — 최근 10경기의 승패, 최신이 앞. */
  recentForm: ('W' | 'L' | 'D')[];
  /** 가장 긴 연승. 오늘 자랑할 거리가 있으면 이거다. */
  bestStreak: number;
  currentStreak: number;
}

const emptyTally = (): Tally => ({ games: 0, wins: 0, losses: 0, rate: 0, elapsedMs: 0, points: 0, innings: 0 });

/** 끝나지 않은 게임은 통계에 넣지 않는다. 중간에 접은 판까지 승률에 세면 숫자가 거짓말이 된다. */
const isFinished = (game: GameSummary) => Boolean(game.finishedAt);

const startOfToday = (now: number) => {
  const date = new Date(now);
  date.setHours(0, 0, 0, 0);
  return date.getTime();
};

const startOfMonth = (now: number) => {
  const date = new Date(now);
  date.setDate(1);
  date.setHours(0, 0, 0, 0);
  return date.getTime();
};

function add(tally: Tally, game: GameSummary): Tally {
  const me = game.me ?? 'white';
  const won = game.winner === me;
  const lost = game.winner === other(me);
  return {
    games: tally.games + 1,
    wins: tally.wins + (won ? 1 : 0),
    losses: tally.losses + (lost ? 1 : 0),
    rate: 0, // 마지막에 한 번만 계산한다.
    elapsedMs: tally.elapsedMs + (game.elapsedMs ?? 0),
    points: tally.points + (game.players[me]?.score ?? 0),
    innings: tally.innings + (game.inning ?? 0),
  };
}

const settle = (tally: Tally): Tally => ({
  ...tally,
  // 무승부는 분모에는 남기고 분자에는 넣지 않는다. 진 것도 이긴 것도 아니라서.
  rate: tally.games === 0 ? 0 : tally.wins / tally.games,
});

export function computeStats(games: GameSummary[], now = Date.now()): Stats {
  const finished = games.filter(isFinished).sort((a, b) => b.startedAt - a.startedAt);

  const todayFrom = startOfToday(now);
  const monthFrom = startOfMonth(now);

  let today = emptyTally();
  let month = emptyTally();
  let all = emptyTally();

  const opponents = new Map<string, OpponentTally>();
  const kinds = new Map<GameKind, KindTally>();

  for (const game of finished) {
    all = add(all, game);
    if (game.startedAt >= monthFrom) month = add(month, game);
    if (game.startedAt >= todayFrom) today = add(today, game);

    /*
      상대별.

      셋이 친 판에는 상대가 둘이다. 그 판을 한 사람에게만 달아 두면 나머지 한 사람과는
      친 적이 없는 것이 되므로, 나를 뺀 모두에게 같은 판을 단다. 그래서 상대별 게임 수의
      합은 전체 게임 수보다 클 수 있다 — 한 판이 두 사람과의 판이기 때문이다.
    */
    const me = game.me ?? 'white';
    for (const side of sides(game)) {
      if (side === me) continue;
      const opponentName = (game.players[side]?.name ?? '상대').trim() || '상대';
      const previous =
        opponents.get(opponentName) ?? { ...emptyTally(), name: opponentName, lastPlayedAt: 0 };
      opponents.set(opponentName, {
        ...add(previous, game),
        name: opponentName,
        lastPlayedAt: Math.max(previous.lastPlayedAt, game.startedAt),
      });
    }

    const kindKey = game.kind;
    const previousKind =
      kinds.get(kindKey) ?? { ...emptyTally(), kind: kindKey, label: kindInfo(kindKey).label };
    kinds.set(kindKey, { ...add(previousKind, game), kind: kindKey, label: kindInfo(kindKey).label });
  }

  const form = finished.slice(0, 10).map((game): 'W' | 'L' | 'D' => {
    const me = game.me ?? 'white';
    if (game.winner === me) return 'W';
    if (game.winner === other(me)) return 'L';
    return 'D';
  });

  return {
    today: settle(today),
    month: settle(month),
    all: settle(all),
    opponents: [...opponents.values()]
      .map((entry): OpponentTally => ({ ...entry, ...settle(entry) }))
      .sort((a, b) => b.games - a.games || b.lastPlayedAt - a.lastPlayedAt),
    // 친 종목만. 0게임짜리 줄은 "안 쳤다"는 정보이기 전에, 실제로 친 종목을 찾을 때
    // 눈이 건너뛰어야 하는 줄이다 — 종목이 셋뿐이라 없어도 무엇이 빠졌는지 알 수 있다.
    kinds: GAME_KINDS.filter((info) => kinds.has(info.id)).map((info): KindTally => {
      const entry = kinds.get(info.id)!;
      return { ...entry, ...settle(entry) };
    }),
    recentForm: form,
    bestStreak: longestStreak(finished),
    currentStreak: currentStreak(finished),
  };
}

/** 최신이 앞인 배열에서, 가장 긴 연승. */
function longestStreak(finished: GameSummary[]): number {
  let best = 0;
  let run = 0;
  for (const game of [...finished].reverse()) {
    const won = game.winner === (game.me ?? 'white');
    run = won ? run + 1 : 0;
    if (run > best) best = run;
  }
  return best;
}

/** 지금 이어지고 있는 연승. 최근 경기부터 지거나 비길 때까지. */
function currentStreak(finished: GameSummary[]): number {
  let run = 0;
  for (const game of finished) {
    if (game.winner !== (game.me ?? 'white')) break;
    run++;
  }
  return run;
}

/* 달력 -------------------------------------------------------------- */

const pad = (value: number) => String(value).padStart(2, '0');

/**
 * `2026-08-09`. 기기의 지역 시간 기준이다.
 *
 * `toISOString()`을 쓰지 않는 이유가 여기 있다 — 그건 UTC로 자르므로, 새벽 한 시에
 * 친 게임이 한국에서는 어제 날짜 칸에 들어간다. 달력에서 그건 곧바로 눈에 띄는 오류다.
 */
export const dayKey = (at: number | Date): string => {
  const date = at instanceof Date ? at : new Date(at);
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}`;
};

/** `2026-08`. */
export const monthKey = (at: number | Date): string => dayKey(at).slice(0, 7);

export interface DayCell {
  /** 그 달의 날짜가 아니면 `null` — 앞뒤 빈 칸이다. */
  key: string | null;
  day: number;
  games: number;
  wins: number;
}

/** 하루에 몇 게임을 쳤고 몇 번 이겼는지. 달력 칸이 읽는 값이다. */
export function tallyByDay(games: GameSummary[]): Map<string, { games: number; wins: number }> {
  const out = new Map<string, { games: number; wins: number }>();
  for (const game of games) {
    if (!game.finishedAt) continue;
    const key = dayKey(game.startedAt);
    const entry = out.get(key) ?? { games: 0, wins: 0 };
    entry.games += 1;
    if (game.winner === (game.me ?? 'white')) entry.wins += 1;
    out.set(key, entry);
  }
  return out;
}

/**
 * 한 달치 달력 격자. 일요일 시작, 6주 고정.
 *
 * 주 수를 달마다 바꾸면 달을 넘길 때 아래 카드들이 위아래로 뛴다. 한 줄이 비더라도
 * 높이가 일정한 편이 손가락으로 넘기기에 낫다.
 */
export function monthGrid(year: number, month: number, tally: Map<string, { games: number; wins: number }>): DayCell[] {
  const first = new Date(year, month, 1);
  const daysInMonth = new Date(year, month + 1, 0).getDate();
  const lead = first.getDay();

  const cells: DayCell[] = [];
  for (let i = 0; i < 42; i++) {
    const day = i - lead + 1;
    if (day < 1 || day > daysInMonth) {
      cells.push({ key: null, day: 0, games: 0, wins: 0 });
      continue;
    }
    const key = `${year}-${pad(month + 1)}-${pad(day)}`;
    const entry = tally.get(key);
    cells.push({ key, day, games: entry?.games ?? 0, wins: entry?.wins ?? 0 });
  }
  return cells;
}

export const WEEKDAYS = ['일', '월', '화', '수', '목', '금', '토'] as const;

/* 표시용 ------------------------------------------------------------ */

export const percent = (rate: number) => `${Math.round(rate * 100)}%`;

/** `1시간 23분` — 통계에서는 초까지 볼 이유가 없다. */
export function humanDuration(ms: number): string {
  const minutes = Math.round(ms / 60000);
  if (minutes < 60) return `${minutes}분`;
  const hours = Math.floor(minutes / 60);
  const rest = minutes % 60;
  return rest === 0 ? `${hours}시간` : `${hours}시간 ${rest}분`;
}

/** 애버리지 — 이닝당 득점. 당구장에서 실력을 말할 때 쓰는 그 숫자다. */
export const averageOf = (tally: Tally) => (tally.innings === 0 ? 0 : tally.points / tally.innings);

export const sideLabel = (side: Side) => (side === 'white' ? '흰 공' : '노란 공');

/* 다시 만날 사람들 --------------------------------------------------- */

/**
 * 로비에 세울 상대 목록.
 *
 * `Stats.opponents`와 다른 값이다. 저쪽은 "누구와 많이 쳤나"를 세는 통계라 많이 친
 * 순서로 늘어서지만, 로비에서 필요한 것은 "지금 앞에 있는 사람"이고 그건 대개 어제
 * 친 사람이다 — 그래서 최근 순으로, 몇 명만.
 *
 * 이름과 함께 그때의 설정을 들고 온다. 같은 사람과는 대개 같은 종목을 같은 핸디로
 * 치므로, 이름을 고르는 것이 곧 판을 차리는 것이 된다. 핸디는 실력이 바뀌면 달라지지만
 * 그때도 마지막 값이 손대기 좋은 출발점이다.
 */
export interface OpponentCard {
  name: string;
  games: number;
  wins: number;
  losses: number;
  lastPlayedAt: number;
  last: {
    kind: GameKind;
    /** 내 핸디와 상대 핸디. */
    mine: number;
    theirs: number;
    cushion: number;
  };
}

/** 사람이 적은 이름이 아니라 앱이 대신 붙인 이름들. */
const DEFAULT_NAMES = new Set(['상대', '흰 공', '노란 공', '빨간 공', '파란 공']);

export function recentOpponents(games: GameSummary[], limit = 6): OpponentCard[] {
  const cards = new Map<string, OpponentCard>();

  // 최근 것부터 본다. 그래야 `last`가 늘 마지막 판의 것이 된다 — 뒤에 오는 옛 판은
  // 세기만 하고 설정은 덮어쓰지 않는다.
  const ordered = [...games].sort((a, b) => b.startedAt - a.startedAt);

  for (const game of ordered) {
    const me = game.me ?? 'white';
    for (const side of sides(game)) {
      if (side === me) continue;
      const them = game.players[side];
      const name = (them?.name ?? '').trim();
    // 이름을 적지 않은 판은 세지 않는다. 앱이 대신 붙인 이름들이라, 그걸 목록에 세우면
    // "노란 공과 한 판 더"가 된다 — 그런 사람은 없다.
      if (!name || DEFAULT_NAMES.has(name)) continue;

      const found = cards.get(name);
      if (found) {
        cards.set(name, {
          ...found,
          games: found.games + 1,
          wins: found.wins + (game.winner === me ? 1 : 0),
          losses: found.losses + (game.winner && game.winner !== me ? 1 : 0),
        });
        continue;
      }

      cards.set(name, {
        name,
        games: 1,
        wins: game.winner === me ? 1 : 0,
        losses: game.winner && game.winner !== me ? 1 : 0,
        lastPlayedAt: game.startedAt,
        last: {
          kind: game.kind,
          mine: game.players[me]?.target ?? 20,
          theirs: them?.target ?? 20,
          cushion: game.lastCushion ?? 0,
        },
      });
    }
  }

  return [...cards.values()].sort((a, b) => b.lastPlayedAt - a.lastPlayedAt).slice(0, limit);
}
