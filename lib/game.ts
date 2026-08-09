/**
 * 게임 규칙. UI도, 저장소도, Firebase도 모르는 순수 함수들.
 *
 * 점수판 앱에서 진짜로 틀리면 안 되는 건 화면이 아니라 여기다: 되돌리기가 정확한지,
 * 이닝이 언제 올라가는지, 목표 점수에 닿은 순간이 언제인지. 그래서 상태 전이는 전부
 * 리듀서 하나로 모아 두었고, 화면은 이걸 그리기만 한다.
 */

export type Side = 'white' | 'yellow';

export type GameKind = 'four' | 'three' | 'pocket';

export interface GameKindInfo {
  id: GameKind;
  label: string;
  /** 처음 여는 화면에서 제안할 목표 점수. 핸디는 어차피 각자 조정한다. */
  defaultTarget: number;
  /** 한 번에 올릴 수 있는 점수의 최대치 — 그 이상은 숫자를 직접 입력한다. */
  quickStep: number;
  hint: string;
}

export const GAME_KINDS: readonly GameKindInfo[] = [
  { id: 'four', label: '4구', defaultTarget: 20, quickStep: 3, hint: '적구 두 개를 다 맞히면 1점' },
  { id: 'three', label: '3구', defaultTarget: 15, quickStep: 3, hint: '쿠션 3번 이상, 성공하면 1점' },
  { id: 'pocket', label: '포켓볼', defaultTarget: 7, quickStep: 3, hint: '넣은 공 수만큼' },
];

export const kindInfo = (kind: GameKind): GameKindInfo =>
  GAME_KINDS.find((entry) => entry.id === kind) ?? GAME_KINDS[0];

export interface PlayerState {
  name: string;
  /** 핸디 — 이 사람이 몇 점을 쳐야 이기는지. 두 사람이 서로 다른 게 정상이다. */
  target: number;
  score: number;
}

/** 되돌리기가 있는 이유: 잘못 누른 걸 빼기 버튼으로 고치면 이닝 수가 틀어진다. */
export interface ScoreEntry {
  at: number;
  side: Side;
  delta: number;
  /** 적용 후 점수. 되돌릴 때 계산하지 않고 그대로 복원한다. */
  scoreAfter: number;
  /** 이 득점이 차례를 넘겼는지 — 되돌리면 차례도 같이 돌아와야 한다. */
  turnBefore: Side;
  inningBefore: number;
}

export interface GameState {
  id: string;
  kind: GameKind;
  /**
   * 둘 중 어느 쪽이 이 폰의 주인인지.
   *
   * 통계가 "내 승률"을 낼 수 있는 유일한 근거다. 이름으로 맞추는 방법도 있지만, 같은
   * 사람이 이름을 다르게 적는 날이 반드시 오고 그러면 승률이 조용히 틀린다.
   */
  me: Side;
  /**
   * 목표 점수를 채운 뒤 쿠션으로 더 쳐야 하는 점수 (4구 관습).
   *
   * 0이면 목표 점수에 닿는 순간 끝난다. 1 이상이면 20점을 다 친 다음부터가 쿠션
   * 구간이고, 거기서 그만큼을 더 넣어야 이긴다 — 즉 실제로 이기는 점수는 `target +
   * lastCushion`이다. 목표 점수 *안의* 마지막 몇 점이 아니라 그 *뒤에* 붙는 점수라는
   * 게 핵심이다.
   *
   * 쿠션을 실제로 거쳤는지는 테이블에서만 알 수 있으므로 판정은 사람이 한다. 점수판이
   * 하는 일은 지금이 쿠션 구간이라는 것과 몇 점이 남았는지를 크게 보여 주는 것이다.
   */
  lastCushion: number;
  startedAt: number;
  /** 누적 진행 시간(ms). 일시정지가 있으므로 벽시계 차이로 계산하지 않는다. */
  elapsedMs: number;
  running: boolean;
  turn: Side;
  inning: number;
  players: Record<Side, PlayerState>;
  history: ScoreEntry[];
  finishedAt?: number;
  winner?: Side;
}

export interface NewGameOptions {
  kind: GameKind;
  white: { name: string; target: number };
  yellow: { name: string; target: number };
  /** 선공. 흰 공이 기본이지만 로비에서 바꿀 수 있다. */
  first?: Side;
  /** 이 폰의 주인이 잡은 공. 통계가 내 승률을 낼 때 쓴다. */
  me?: Side;
  /** 목표 점수를 채운 뒤 쿠션으로 더 쳐야 하는 점수. 4구에서만 쓰고, 기본은 없음. */
  lastCushion?: number;
  now?: number;
  id?: string;
}

export function createGame({
  kind,
  white,
  yellow,
  first = 'white',
  me = 'white',
  lastCushion = 0,
  now = Date.now(),
  id,
}: NewGameOptions): GameState {
  return {
    id: id ?? `g-${now.toString(36)}-${Math.random().toString(36).slice(2, 7)}`,
    kind,
    me,
    lastCushion: Math.max(0, Math.round(lastCushion)),
    startedAt: now,
    elapsedMs: 0,
    running: true,
    turn: first,
    inning: 1,
    players: {
      white: { name: white.name.trim() || '흰 공', target: Math.max(1, white.target), score: 0 },
      yellow: { name: yellow.name.trim() || '노란 공', target: Math.max(1, yellow.target), score: 0 },
    },
    history: [],
  };
}

export type GameAction =
  | { type: 'score'; side: Side; delta: number; now?: number }
  | { type: 'setScore'; side: Side; score: number; now?: number }
  | { type: 'turn'; side?: Side }
  | { type: 'undo' }
  | { type: 'tick'; ms: number }
  | { type: 'pause' }
  | { type: 'resume' }
  | { type: 'finish'; now?: number }
  | { type: 'rename'; side: Side; name: string }
  | { type: 'setTarget'; side: Side; target: number }
  | { type: 'setLastCushion'; value: number };

export function reduce(state: GameState, action: GameState | GameAction): GameState {
  switch ((action as GameAction).type) {
    case 'score': {
      const { side, delta, now = Date.now() } = action as Extract<GameAction, { type: 'score' }>;
      if (state.finishedAt || delta === 0) return state;
      return applyScore(state, side, clampScore(state.players[side].score + delta), now);
    }

    case 'setScore': {
      const { side, score, now = Date.now() } = action as Extract<GameAction, { type: 'setScore' }>;
      if (state.finishedAt) return state;
      const next = clampScore(score);
      if (next === state.players[side].score) return state;
      return applyScore(state, side, next, now);
    }

    case 'turn': {
      const { side } = action as Extract<GameAction, { type: 'turn' }>;
      const next = side ?? other(state.turn);
      if (next === state.turn) return state;
      // 한 바퀴 돌아 선공에게 돌아오면 이닝이 하나 올라간다. 그게 이닝의 정의다.
      const inning = next === firstSide(state) ? state.inning + 1 : state.inning;
      return { ...state, turn: next, inning };
    }

    case 'undo': {
      const last = state.history.at(-1);
      if (!last) return state;
      return {
        ...state,
        players: {
          ...state.players,
          [last.side]: { ...state.players[last.side], score: last.scoreAfter - last.delta },
        },
        turn: last.turnBefore,
        inning: last.inningBefore,
        history: state.history.slice(0, -1),
        // 되돌린 게 끝내기 득점이었다면 게임은 다시 진행 중이 된다.
        finishedAt: undefined,
        winner: undefined,
        running: true,
      };
    }

    case 'tick': {
      const { ms } = action as Extract<GameAction, { type: 'tick' }>;
      if (!state.running || state.finishedAt) return state;
      return { ...state, elapsedMs: state.elapsedMs + ms };
    }

    case 'pause':
      return state.running ? { ...state, running: false } : state;

    case 'resume':
      return state.running || state.finishedAt ? state : { ...state, running: true };

    case 'finish': {
      const { now = Date.now() } = action as Extract<GameAction, { type: 'finish' }>;
      if (state.finishedAt) return state;
      return { ...state, running: false, finishedAt: now, winner: leader(state) };
    }

    case 'rename': {
      const { side, name } = action as Extract<GameAction, { type: 'rename' }>;
      return { ...state, players: { ...state.players, [side]: { ...state.players[side], name } } };
    }

    case 'setTarget': {
      const { side, target } = action as Extract<GameAction, { type: 'setTarget' }>;
      return {
        ...state,
        players: { ...state.players, [side]: { ...state.players[side], target: Math.max(1, target) } },
      };
    }

    case 'setLastCushion': {
      const { value } = action as Extract<GameAction, { type: 'setLastCushion' }>;
      return { ...state, lastCushion: Math.max(0, Math.round(value)) };
    }

    default:
      // 저장된 게임을 그대로 불러오는 경우 — 리듀서를 거치게 해서 로딩 경로를 하나로 둔다.
      return action as GameState;
  }
}

function applyScore(state: GameState, side: Side, score: number, now: number): GameState {
  const delta = score - state.players[side].score;
  const players = { ...state.players, [side]: { ...state.players[side], score } };
  const entry: ScoreEntry = {
    at: now,
    side,
    delta,
    scoreAfter: score,
    turnBefore: state.turn,
    inningBefore: state.inning,
  };

  const next: GameState = {
    ...state,
    players,
    // 점수를 넣은 사람이 계속 친다 — 실패해야 차례가 넘어간다. 그래서 득점은 차례를
    // 바꾸지 않고, 대신 누가 치고 있는지는 마지막에 넣은 사람으로 맞춘다.
    turn: delta > 0 ? side : state.turn,
    history: [...state.history, entry],
  };

  // 이기는 점수는 목표 점수 + 쿠션 점수다. 쿠션 규칙이 있으면 20점은 끝이 아니라
  // 쿠션 구간의 시작이므로, 여기서 끝내면 규칙이 없는 것과 같아진다.
  if (players[side].score >= players[side].target + (state.lastCushion ?? 0)) {
    return { ...next, running: false, finishedAt: now, winner: side };
  }
  return next;
}

const clampScore = (value: number) => Math.max(0, Math.min(999, Math.round(value)));

export const other = (side: Side): Side => (side === 'white' ? 'yellow' : 'white');

/** 이 게임의 선공. 첫 이닝을 연 사람이 이닝 경계를 정한다. */
function firstSide(state: GameState): Side {
  return state.history[0]?.turnBefore ?? state.turn;
}

/** 지금 이기고 있는 쪽. 남은 점수가 적은 쪽이고, 같으면 무승부로 흰 공을 반환하지 않는다. */
export function leader(state: GameState): Side | undefined {
  const remaining = (side: Side) => winningScore(state, side) - state.players[side].score;
  const white = remaining('white');
  const yellow = remaining('yellow');
  if (white === yellow) return undefined;
  return white < yellow ? 'white' : 'yellow';
}

/** 이 사람이 실제로 이기는 점수. 쿠션 규칙이 붙으면 목표 점수보다 그만큼 높다. */
export const winningScore = (state: GameState, side: Side) =>
  state.players[side].target + (state.lastCushion ?? 0);

/** 목표 점수까지 남은 점수. 쿠션 구간에 들어오면 0이 된다. */
export const remaining = (state: GameState, side: Side) =>
  Math.max(0, state.players[side].target - state.players[side].score);

/**
 * 지금 이 사람이 쿠션 구간에 있는지 — 목표 점수를 채웠고, 쿠션 점수가 남았는지.
 *
 * 규칙 자체를 앱이 판정할 수는 없다. 공이 쿠션을 거쳤는지는 테이블에서만 알 수 있다.
 * 그래서 하는 일은 하나다: 20점을 채운 순간부터 남은 쿠션 점수를 크게 띄우는 것.
 */
export const needsCushion = (state: GameState, side: Side) =>
  (state.lastCushion ?? 0) > 0 && state.players[side].score >= state.players[side].target;

/** 쿠션으로 더 쳐야 하는 점수. 쿠션 구간이 아니면 0이다. */
export const cushionRemaining = (state: GameState, side: Side) =>
  needsCushion(state, side) ? Math.max(0, winningScore(state, side) - state.players[side].score) : 0;

/**
 * 점수판에 크게 띄울 숫자.
 *
 * 쿠션 구간에 들어가면 0부터 다시 센다. 20점을 채운 사람 자리에 21, 22가 떠 있으면
 * 그게 목표를 넘긴 점수인지 쿠션 점수인지 한눈에 구분되지 않는다 — 숫자를 0으로
 * 되돌리고 위에 "쿠션"이라고 적으면 헷갈릴 여지가 없어진다. 저장되는 점수는 그대로
 * 누적값이고, 바뀌는 건 보이는 방식뿐이다.
 */
export const displayScore = (state: GameState, side: Side) =>
  needsCushion(state, side)
    ? state.players[side].score - state.players[side].target
    : state.players[side].score;

/** `12:34` 또는 한 시간이 넘으면 `1:02:03`. */
export function formatClock(ms: number): string {
  const total = Math.max(0, Math.floor(ms / 1000));
  const seconds = total % 60;
  const minutes = Math.floor(total / 60) % 60;
  const hours = Math.floor(total / 3600);
  const pad = (value: number) => String(value).padStart(2, '0');
  return hours > 0 ? `${hours}:${pad(minutes)}:${pad(seconds)}` : `${pad(minutes)}:${pad(seconds)}`;
}

/**
 * 이 사람이 지금까지 친 이닝 수.
 *
 * 이닝 번호는 판 전체에 하나지만, 그 안에서 두 사람이 친 횟수는 같지 않다. 선공이
 * 3이닝을 시작해 아직 치고 있으면 후공은 2이닝만 친 것이고, 그 상태에서 둘을 같은
 * 수로 나누면 후공의 에버가 실제보다 낮게 나온다. 당구장에서 에버는 자기가 친 이닝으로
 * 나눈 값이므로, 여기서 각자의 몫을 센다.
 */
export function innings(state: GameState, side: Side): number {
  const first = firstSide(state);
  if (side === first) return state.inning;
  // 후공은 선공이 이번 이닝을 아직 넘기지 않았으면 한 이닝 적다.
  return Math.max(0, state.turn === first ? state.inning - 1 : state.inning);
}

/** 한 게임의 평균 애버리지 — 당구장에서 실제로 보는 숫자. 자기가 친 이닝으로 나눈다. */
export function average(state: GameState, side: Side): number {
  const played = innings(state, side);
  if (played === 0) return 0;
  return state.players[side].score / played;
}

/** 저장·전송용으로 줄인 형태. 히스토리 전체를 클라우드에 넣을 이유는 없다. */
export interface GameSummary {
  id: string;
  kind: GameKind;
  me: Side;
  lastCushion?: number;
  startedAt: number;
  finishedAt?: number;
  elapsedMs: number;
  inning: number;
  winner?: Side;
  players: Record<Side, PlayerState>;
}

export const summarize = (state: GameState): GameSummary => ({
  id: state.id,
  kind: state.kind,
  me: state.me ?? 'white',
  lastCushion: state.lastCushion ?? 0,
  startedAt: state.startedAt,
  finishedAt: state.finishedAt,
  elapsedMs: state.elapsedMs,
  inning: state.inning,
  winner: state.winner,
  players: state.players,
});
