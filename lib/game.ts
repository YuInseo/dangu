/**
 * 게임 규칙. UI도, 저장소도, Firebase도 모르는 순수 함수들.
 *
 * 점수판 앱에서 진짜로 틀리면 안 되는 건 화면이 아니라 여기다: 되돌리기가 정확한지,
 * 이닝이 언제 올라가는지, 목표 점수에 닿은 순간이 언제인지. 그래서 상태 전이는 전부
 * 리듀서 하나로 모아 두었고, 화면은 이걸 그리기만 한다.
 */

/**
 * 한 자리를 가리키는 이름.
 *
 * 값이 곧 공 색이다 — `white`, `yellow`, 셋째부터 `red`, `blue`. 색을 따로 들고 다니지
 * 않는 이유는 점수판에서 사람을 구분하는 것이 색이기 때문이다. 판의 배경색도, 이닝
 * 표의 글자색도 이 값 하나에서 나온다.
 *
 * 두 명뿐이던 때에는 `'white' | 'yellow'`였다. 넓힌 자리에 저장된 게임이 그대로
 * 들어맞는다는 게 이 방식의 값어치다: 예전 기록의 `players`는 키가 둘인 객체이고,
 * 지금 모델에서도 정확히 그것이다.
 */
export type Side = string;

/** 자리의 키 — 순서대로 쓴다. 넷을 넘는 판은 점수판 화면이 감당하지 못한다. */
export const SIDES: readonly Side[] = ['white', 'yellow', 'red', 'blue'];

/** 자리마다의 기본 이름. 아무것도 적지 않은 사람은 이렇게 불린다. */
export const SIDE_LABELS: Record<Side, string> = {
  white: '흰 공',
  yellow: '노란 공',
  red: '3번',
  blue: '4번',
};

/**
 * 이 자리가 잡는 공의 색.
 *
 * 테이블에는 수구가 둘뿐이다 — 흰 공과 노란 공. 셋이 쳐도 넷이 쳐도 그건 그대로이고,
 * 세 번째 사람은 없는 색의 공을 잡는 것이 아니라 앞사람이 놓은 공을 이어 잡는다. 그래서
 * 자리 키는 넷이지만 색은 둘이고, 순서대로 번갈아 간다.
 *
 * 키와 색을 따로 두는 이유는 저장된 기록 때문이다. 키는 그 판에서 누가 누구였는지를
 * 가리키는 이름이라 바뀌면 안 되고, 색은 화면이 그리는 방식이라 언제든 바뀔 수 있다.
 */
export const ballOf = (list: readonly Side[], side: Side): 'white' | 'yellow' =>
  list.indexOf(side) % 2 === 1 ? 'yellow' : 'white';

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
  /**
   * 팀전에서 이 자리에 선 사람들.
   *
   * 팀은 점수를 함께 쓴다. 그래서 팀 하나가 자리 하나이고, 그 안에서 누가 칠 차례인지는
   * 이 목록을 돌아가며 정한다 — 자리에 차례가 올 때마다 다음 사람으로 넘어간다. 몇 번째
   * 사람인지를 따로 저장하지 않는 이유는, 그 값이 이미 "이 자리가 몇 이닝을 쳤나"와
   * 같은 수이기 때문이다. 두 벌로 두면 되돌리기에서 어긋난다.
   *
   * 개인전에는 없다.
   */
  members?: string[];
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
  /**
   * 선공. 이닝의 경계를 정하는 기준이다.
   *
   * 예전에는 이 값을 저장하지 않고 "첫 득점을 남긴 사람"으로 되짚었다. 그러면 아무도
   * 아직 득점하지 않은 동안 기준이 지금 차례를 따라 움직여서, 차례를 아무리 넘겨도
   * 이닝이 올라가지 않는다. 기준은 게임이 시작될 때 정해지는 값이므로 그때 적어 둔다.
   */
  first: Side;
  /**
   * 치는 순서.
   *
   * 이 목록의 자리 순서대로 돌아가고, 한 바퀴 돌아 선공에게 오면 이닝이 하나 올라간다.
   * 두 명이던 시절에 저장된 게임에는 이 값이 없다 — 그때는 순서가 `['white','yellow']`
   * 하나뿐이었으므로, 읽는 자리에서 그 값을 채워 준다.
   */
  order?: Side[];
  /**
   * 후구를 쓰는지.
   *
   * 선공은 매 이닝 먼저 친다. 그래서 선공이 목표를 채우는 순간 끝내 버리면 선공이 한
   * 번 더 친 채로 판이 닫힌다. 후구는 그것을 메운다: 목표를 채워도 그 자리에서 끝나지
   * 않고, 승부는 이닝이 온전히 끝난 자리에서만 난다.
   *
   * 후공이 따라붙으면 비기는 게 아니라 매치포인트로 이어진다 — 둘 다 채운 채로 한
   * 이닝씩 더 치고, 한 이닝에서 한쪽이 더 친 순간 끝난다.
   */
  equalizer: boolean;
  /**
   * 뒷빡을 쓰는지.
   *
   * 4구에서 상대 수구를 맞히면 잘못 친 것이다. 이 규칙을 쓰는 판에서 바뀌는 것은 둘이다.
   * 상대 판을 누르면 상대에게 한 점이 가는 대신 차례만 넘어가고, 점수의 바닥이 열려
   * −1 버튼이 0에서도 눌린다 — 몇 점을 물릴지는 치는 사람들이 정하기 때문이다.
   *
   * 앱이 따로 세어 두는 것은 없다. 뒷빡을 몇 번 냈는지는 점수에 이미 들어 있다.
   */
  foul: boolean;
  /**
   * 지금 차례가 시작된 시점 — 벽시계가 아니라 `elapsedMs` 위의 눈금이다.
   *
   * 벽시계로 재면 일시정지한 동안에도 시간이 흐른다. 담배 한 대 피우고 오면 샷 클락이
   * 이미 빨간색인 점수판은 아무 쓸모가 없다. 게임 시계를 기준으로 재면 멈춘 동안은
   * 함께 멈춘다.
   */
  turnAt: number;
  inning: number;
  players: Record<Side, PlayerState>;
  history: ScoreEntry[];
  finishedAt?: number;
  winner?: Side;
  /** 이 판에 남긴 메모와 그림. 없으면 없는 대로 — 예전 기록에는 이 값이 없다. */
  notes?: NotePage[];
}

/**
 * 노트 한 장.
 *
 * 한 장에 글과 그림이 같이 있다. 처음에는 장마다 글이거나 그림이게 했는데, 실제로
 * 적는 것은 "순서를 적고 그 옆에 자리를 그리는" 식이라 둘을 갈라 두면 한 이야기가 두
 * 장으로 찢어진다. 삼성 노트가 그렇게 하는 이유이기도 하다.
 *
 * 그림은 점의 목록으로 남긴다. 화면 이미지로 저장하면 폰마다 크기가 다른 만큼 다시
 * 그릴 수 없고, 한 장이 수십 KB라 기록 오십 판이면 저장소를 넘긴다. 좌표는 0~1로
 * 정규화해서 어떤 크기에도 같은 그림이 나온다.
 */
export interface NotePage {
  id: string;
  text?: string;
  strokes?: Stroke[];
  /** 판 위에 놓은 공들. 배치를 그리는 것이 이 노트의 절반이다. */
  balls?: Ball[];
  /** 당구대 대신 빈 종이로 볼지. 기본은 당구대다 — 여기 적히는 것의 대부분이 배치다. */
  plain?: boolean;
  /**
   * 어느 종목의 공인지. 기본은 4구.
   *
   * 공과 판의 크기가 종목마다 다르고, 그 둘의 *비율*이 배치를 읽는 눈금이 된다 —
   * 3쿠션은 대대(2840mm)에 61.5mm 공이라 판 긴 변의 2.2%, 4구는 중대(2540mm)에 65.5mm
   * 공이라 2.6%다. 같은 자리에 놓인 공도 종목이 다르면 다른 그림이다.
   */
  ball?: 'three' | 'four';
}

/**
 * 판 위의 공 하나.
 *
 * 색은 한 글자다(`w` 흰, `y` 노랑, `r` 빨강) — 획과 같은 이유로, 한 장에 여럿이고 그대로
 * JSON이 된다. 자리는 획과 같은 0~1 좌표라 어떤 화면에서도 같은 배치가 나온다.
 */
export interface Ball {
  c: 'w' | 'y' | 'r';
  x: number;
  y: number;
  /**
   * 당점 — 이 공의 어디를 치는지.
   *
   * 공 한가운데가 `(0, 0)`이고 가장자리가 반지름 1이다. 판 위의 자리(`x`, `y`)와 달리
   * 공 안에서의 자리라, 공을 옮겨도 당점은 그대로 따라간다.
   */
  t?: { x: number; y: number };
}

/**
 * 한 번 그은 선.
 *
 * 이름이 한 글자씩인 것은 이게 그대로 JSON이 되기 때문이다 — 한 장에 획이 수백 개고,
 * `points`/`color`/`width`로 적으면 그 이름들이 그림보다 무겁다.
 *
 * `number[]`도 받는다. 색과 두께가 생기기 전에 그린 그림들이 그 모양이고, 남의 기록을
 * 우리 사정으로 버릴 수는 없다 — 읽을 때 기본 펜으로 친다.
 */
export interface Ink {
  /** 점들. `[x, y, x, y, …]`, 0~1로 정규화. */
  p: number[];
  /** 색. */
  c: string;
  /** 두께 — 칸의 너비에 대한 비율이라 화면 크기가 달라도 굵기가 같다. */
  w: number;
  /** 형광펜이면 참. 반투명하게 그리고 글씨 위에 얹힌다. */
  h?: boolean;
  /** 진하기(0~1). 없으면 형광펜은 0.35, 펜은 1. 연필과 형광펜 진하기가 이 값을 쓴다. */
  a?: number;
}

export type Stroke = number[] | Ink;

/** 예전 모양(`number[]`)을 지금 모양으로. 읽는 쪽 어디서나 이걸 거친다. */
export const inkOf = (stroke: Stroke): Ink =>
  Array.isArray(stroke) ? { p: stroke, c: '#f3f4f6', w: 0.006 } : stroke;

/**
 * 저장된 노트를 지금 모양으로 맞춘다.
 *
 * 예전에는 장이 `{kind:'text'}`거나 `{kind:'draw'}`였다. 그 기록도 그대로 열려야 하므로
 * 읽는 자리에서 한 번 옮긴다 — 저장된 데이터를 통째로 고치는 이사보다, 읽을 때마다
 * 값싸게 맞춰 주는 쪽이 되돌리기 쉽다.
 */
export function readNotes(pages: readonly any[] | undefined): NotePage[] {
  return (pages ?? []).map((page) => {
    /*
     * 없는 값은 키를 만들지 않는다.
     *
     * `{ text: undefined }`는 "없다"가 아니라 "undefined라는 값이 있다"이다. 이 객체는
     * 아일랜드 props로 직렬화되는데, 그 검사기는 값을 하나씩 훑다가 undefined를 만나면
     * 거기서 죽는다 — 그러면 노트 화면이 통째로 사라진다. 실제로 그렇게 사라졌었다.
     */
    const next: NotePage = { id: String(page?.id ?? `n-${Math.random().toString(36).slice(2, 8)}`) };
    if (typeof page?.text === 'string') next.text = page.text;
    if (Array.isArray(page?.strokes)) next.strokes = page.strokes as Stroke[];
    if (Array.isArray(page?.balls)) next.balls = page.balls as Ball[];
    if (page?.plain === true) next.plain = true;
    if (page?.ball === 'three' || page?.ball === 'four') next.ball = page.ball;
    return next;
  });
}

/** 판에 앉는 한 자리. 팀전이면 `members`에 그 팀 사람들이 들어간다. */
export interface Seat {
  name: string;
  target: number;
  members?: string[];
}

export interface NewGameOptions {
  kind: GameKind;
  /**
   * 셋 이상이 치거나 팀으로 칠 때의 자리들 — 치는 순서대로.
   *
   * 주면 `white`/`yellow`는 무시된다. 둘이 치는 판은 예전 그대로 두 값으로 만들 수
   * 있다: 그게 이 앱에서 압도적으로 흔한 경우이고, 거기까지 배열로 적게 하면 흔한
   * 일이 번거로워진다.
   */
  seats?: Seat[];
  white?: { name: string; target: number };
  yellow?: { name: string; target: number };
  /** 선공. 흰 공이 기본이지만 로비에서 바꿀 수 있다. */
  first?: Side;
  /** 이 폰의 주인이 잡은 공. 통계가 내 승률을 낼 때 쓴다. */
  me?: Side;
  /** 목표 점수를 채운 뒤 쿠션으로 더 쳐야 하는 점수. 4구에서만 쓰고, 기본은 없음. */
  lastCushion?: number;
  /** 후구 — 선공이 먼저 채우면 후공에게 마지막 한 차례. */
  equalizer?: boolean;
  /** 뒷빡 — 상대 수구를 맞히면 차례를 넘기고 횟수를 센다. */
  foul?: boolean;
  now?: number;
  id?: string;
}

export function createGame({
  kind,
  seats,
  white,
  yellow,
  first,
  me = 'white',
  lastCushion = 0,
  equalizer = false,
  foul = false,
  now = Date.now(),
  id,
}: NewGameOptions): GameState {
  const list: Seat[] = seats?.length
    ? seats
    : [
        { name: white?.name ?? '', target: white?.target ?? 20 },
        { name: yellow?.name ?? '', target: yellow?.target ?? 20 },
      ];

  const order = list.map((_, index) => SIDES[index] ?? `p${index + 1}`);
  const players: Record<Side, PlayerState> = {};
  list.forEach((seat, index) => {
    const key = order[index];
    const next: PlayerState = {
      name: seat.name.trim() || SIDE_LABELS[key] || `${index + 1}번`,
      target: Math.max(1, Math.round(seat.target)),
      score: 0,
    };
    // 값이 없는 키는 아예 만들지 않는다 — 아일랜드 직렬화가 undefined에서 죽는다.
    const members = seat.members?.map((name) => name.trim()).filter(Boolean);
    if (members?.length) next.members = members;
    players[key] = next;
  });

  const opening = first && order.includes(first) ? first : order[0];

  return {
    id: id ?? `g-${now.toString(36)}-${Math.random().toString(36).slice(2, 7)}`,
    kind,
    me,
    lastCushion: Math.max(0, Math.round(lastCushion)),
    startedAt: now,
    elapsedMs: 0,
    running: true,
    turn: opening,
    first: opening,
    order,
    equalizer,
    foul,
    turnAt: 0,
    inning: 1,
    players,
    history: [],
  };
}

export type GameAction =
  | { type: 'score'; side: Side; delta: number; now?: number }
  | { type: 'setScore'; side: Side; score: number; now?: number }
  | { type: 'turn'; side?: Side; now?: number }
  | { type: 'undo' }
  | { type: 'tick'; ms: number }
  | { type: 'pause' }
  | { type: 'resume' }
  | { type: 'finish'; now?: number }
  | { type: 'rename'; side: Side; name: string }
  | { type: 'setTarget'; side: Side; target: number }
  | { type: 'setLastCushion'; value: number }
  | { type: 'notes'; pages: NotePage[] };

export function reduce(state: GameState, action: GameState | GameAction): GameState {
  switch ((action as GameAction).type) {
    case 'score': {
      const { side, delta, now = Date.now() } = action as Extract<GameAction, { type: 'score' }>;
      if (state.finishedAt || delta === 0) return state;
      return applyScore(state, side, clampScore(state.players[side].score + delta, scoreFloor(state)), now);
    }

    case 'setScore': {
      const { side, score, now = Date.now() } = action as Extract<GameAction, { type: 'setScore' }>;
      if (state.finishedAt) return state;
      const next = clampScore(score, scoreFloor(state));
      if (next === state.players[side].score) return state;
      return applyScore(state, side, next, now);
    }

    case 'turn': {
      const { side, now = Date.now() } = action as Extract<GameAction, { type: 'turn' }>;
      const next = side ?? nextSide(state, state.turn);
      if (next === state.turn) return state;
      return handOver(state, next, now);
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
        // 되돌린 뒤의 샷 클락은 지금부터 다시 센다. 되돌리기는 잘못 누른 것을 고치는
        // 동작이고, 그 사이에 흐른 시간을 누구의 것으로 볼지는 정할 방법이 없다.
        turnAt: state.elapsedMs,
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

    /*
     * 노트는 통째로 갈아 끼운다.
     *
     * 한 획을 그을 때마다 리듀서가 도는 것이 아니라, 노트 화면이 자기 상태를 들고 있다가
     * 손을 뗄 때 지금의 전부를 넘긴다. 그림 한 장이 획 수백 개인데 그걸 한 획씩 여기로
     * 흘리면 그때마다 게임 전체가 저장된다 — 점수판이 그림 때문에 느려질 이유는 없다.
     */
    case 'notes': {
      const { pages } = action as Extract<GameAction, { type: 'notes' }>;
      return { ...state, notes: pages };
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
    //
    // 그 "맞추는" 일도 차례를 옮기는 것이므로 이닝을 함께 본다. 판을 눌러 점수를 올리는
    // 지금은 상대 차례에 자기 판을 누르는 것이 차례를 넘기는 가장 흔한 방법이 되었고,
    // 여기서 이닝을 세지 않으면 둘이 번갈아 쳐도 1이닝에 머문다.
    ...(delta > 0 ? moveTurn(state, side) : { turn: state.turn, inning: state.inning }),
    history: [...state.history, entry],
  };

  // 이기는 점수는 목표 점수 + 쿠션 점수다. 쿠션 규칙이 있으면 20점은 끝이 아니라
  // 쿠션 구간의 시작이므로, 여기서 끝내면 규칙이 없는 것과 같아진다.
  if (players[side].score >= players[side].target + (state.lastCushion ?? 0)) {
    // 후구를 쓰지 않으면 채우는 순간이 곧 끝이다.
    if (!state.equalizer) {
      return { ...next, running: false, finishedAt: now, winner: side };
    }

    // 후구를 쓰면 여기서 끝나지 않는다. 목표를 채운 사람은 자기 차례를 마치고 상대에게
    // 넘기며, 승부는 이닝이 끝나는 자리에서만 난다.
    //
    // 선공이 채웠으면 그 넘김이 후구 — 후공에게 주는 마지막 한 차례다. 후공이 채웠으면
    // 그 넘김이 곧 이닝의 끝이라 그 자리에서 판정이 난다. 두 경우를 나눠 쓰지 않는
    // 이유는 실제로 같은 동작이기 때문이다.
    return handOver(next, nextSide(state, side), now);
  }
  return next;
}

/**
 * 점수가 들어갈 수 있는 범위.
 *
 * 아래 끝이 판마다 다르다. 보통은 0이다 — 점수는 0부터 올라가는 것이고, −1은 잘못
 * 올린 것을 되돌리는 버튼이라 되돌릴 것이 없으면 할 일도 없다. 뒷빡을 쓰는 판에서는
 * 아래가 열린다. 뒷빡을 몇 점으로 물릴지는 치는 사람들이 정하고, 그걸 −1로 직접
 * 빼는 사람에게 0은 벽이 된다.
 */
const clampScore = (value: number, floor = 0) =>
  Math.max(floor, Math.min(999, Math.round(value)));

/** 이 판에서 점수가 내려갈 수 있는 바닥. */
export const scoreFloor = (state: GameState) => (state.foul ? -999 : 0);

/**
 * 이 판의 자리들 — 치는 순서대로.
 *
 * 두 명이던 시절에 저장된 게임에는 `order`가 없다. 그때는 순서가 하나뿐이었으므로
 * 여기서 채워 준다. 저장된 데이터를 옮기는 이사 대신 읽는 자리에서 맞추는 쪽을 고른
 * 이유는, 그 이사가 실패하면 지난 기록이 사라지기 때문이다.
 */
export function sides(state: { order?: Side[]; players: Record<Side, PlayerState> }): Side[] {
  if (state.order?.length) return state.order;
  const keys = Object.keys(state.players ?? {});
  return keys.length ? keys : ['white', 'yellow'];
}

/** 이 자리 다음에 칠 사람. 마지막이면 처음으로 돌아온다. */
export function nextSide(state: GameState, side: Side): Side {
  const list = sides(state);
  const at = list.indexOf(side);
  return list[(at + 1) % list.length] ?? list[0];
}

/**
 * 두 명짜리 판에서 "상대".
 *
 * 셋 이상인 판에는 상대가 하나가 아니므로 이 함수로는 답할 수 없다. 그래도 남겨 둔
 * 이유는 기록이 그렇기 때문이다 — 지난 기록의 승률과 상대별 통계는 전부 둘이 친 판의
 * 것이고, 거기서는 이 물음이 늘 뜻이 있다.
 */
export const other = (side: Side): Side => (side === 'white' ? 'yellow' : 'white');

/**
 * 팀전에서 지금 칠 사람의 이름.
 *
 * 팀은 점수를 함께 쓰므로 자리가 하나다. 그 안에서 누구 차례인지는 이 자리가 몇 이닝을
 * 쳤는지로 정한다 — 차례가 돌아올 때마다 다음 사람이다. 몇 번째인지를 따로 저장하지
 * 않는 이유는 그 값이 이미 이닝 수와 같은 수이고, 두 벌로 두면 되돌리기에서 어긋나기
 * 때문이다.
 */
export function shooter(state: GameState, side: Side): string {
  const player = state.players[side];
  const roster = player?.members;
  if (!roster?.length) return player?.name ?? '';
  const played = Math.max(0, innings(state, side) - 1);
  return roster[played % roster.length] ?? roster[0];
}

/**
 * 차례를 옮긴다. 그 이동이 이닝 경계이면 이닝도 하나 올린다.
 *
 * 차례가 바뀌는 길이 둘이라 한곳으로 모았다: 손으로 넘기는 것과, 상대 차례에 누군가
 * 점수를 올려 자연히 넘어가는 것. 앞엣것만 이닝을 세던 동안 뒤엣것으로 치는 사람들의
 * 이닝은 영영 1이었다.
 */
function moveTurn(state: GameState, next: Side): { turn: Side; inning: number; turnAt: number } {
  if (next === state.turn) {
    return { turn: state.turn, inning: state.inning, turnAt: state.turnAt ?? 0 };
  }
  // 한 바퀴 돌아 선공에게 돌아오면 이닝이 하나 올라간다. 그게 이닝의 정의다.
  const inning = next === firstSide(state) ? state.inning + 1 : state.inning;
  // 차례가 바뀌면 그 사람의 시간은 0부터다.
  return { turn: next, inning, turnAt: state.elapsedMs };
}

/**
 * 차례를 넘긴다. 후구를 쓰는 판이면 이닝이 끝나는 자리에서 승부를 본다.
 *
 * 후구의 요점은 "선공이 한 번 더 친 채로 끝나지 않게" 하는 것이므로, 판정은 언제나
 * 이닝이 온전히 끝난 자리에서만 내려진다. 그 자리는 차례가 선공에게 돌아오는 순간이다.
 */
function handOver(state: GameState, next: Side, now: number): GameState {
  const moved = { ...state, ...moveTurn(state, next) };
  if (!state.equalizer || state.finishedAt) return moved;

  // 이닝이 끝나는 자리인가 — 차례가 선공에게 돌아왔는가.
  if (next !== firstSide(state)) return moved;

  const decided = decide(state);
  if (!decided) return moved;
  return { ...moved, running: false, finishedAt: now, winner: decided };
}

/**
 * 이닝이 끝난 자리에서의 판정.
 *
 * 목표를 넘긴 만큼(`surplus`)이 큰 쪽이 이긴다. 아무도 목표를 채우지 못했으면 아직
 * 판정할 자리가 아니고, 둘이 같으면 한 이닝을 더 친다 — 그게 매치포인트다. 둘 다 채운
 * 뒤로도 같은 점수로 따라오는 동안은 계속 이어지고, 한 이닝에서 한쪽이 더 치는 순간
 * 끝난다.
 *
 * 이 한 가지 규칙이 후구의 두 경우를 모두 덮는다. 선공만 채우고 후공이 못 채운 채로
 * 이닝이 끝나면 선공이 앞서 있으므로 선공이 이기고, 후공이 따라붙었으면 같아지므로
 * 계속된다.
 */
function decide(state: GameState): Side | null {
  const list = sides(state);
  const surplus = (side: Side) => state.players[side].score - winningScore(state, side);
  const best = Math.max(...list.map(surplus));
  if (best < 0) return null;
  // 같은 값이 둘이면 아직 갈리지 않은 것이다. 셋이 치는 판에서도 규칙은 같다.
  const leaders = list.filter((side) => surplus(side) === best);
  return leaders.length === 1 ? leaders[0] : null;
}

/**
 * 후구가 걸린 판에서 지금이 "끝날 수 있는" 국면인지 — 한쪽이라도 목표를 채웠는지.
 *
 * 화면이 그 사실을 말해 주려고 쓴다. 여기서부터는 이닝이 끝날 때마다 승부가 갈릴 수
 * 있으므로, 치는 사람이 그걸 모르고 있으면 안 된다.
 */
export const inDecider = (state: GameState) =>
  Boolean(state.equalizer) &&
  !state.finishedAt &&
  sides(state).some((side) => state.players[side].score >= winningScore(state, side));

/** 지금 차례가 시작된 뒤로 흐른 시간(ms). */
export const turnElapsed = (state: GameState) => Math.max(0, state.elapsedMs - (state.turnAt ?? 0));

/** 이 게임의 선공. 첫 이닝을 연 사람이 이닝 경계를 정한다. */
function firstSide(state: GameState): Side {
  // 이 값이 생기기 전에 저장된 게임에는 없다. 그때는 첫 득점이 남긴 흔적으로 되짚는다.
  return state.first ?? state.history[0]?.turnBefore ?? state.turn;
}

/** 지금 이기고 있는 쪽. 남은 점수가 적은 쪽이고, 같으면 무승부로 흰 공을 반환하지 않는다. */
export function leader(state: GameState): Side | undefined {
  const list = sides(state);
  const left = (side: Side) => winningScore(state, side) - state.players[side].score;
  const best = Math.min(...list.map(left));
  const leaders = list.filter((side) => left(side) === best);
  // 나란히 앞선 사람이 둘이면 무승부다 — 아무도 반환하지 않는다.
  return leaders.length === 1 ? leaders[0] : undefined;
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
  const list = sides(state);
  const start = Math.max(0, list.indexOf(firstSide(state)));
  // 선공을 0번으로 놓았을 때의 순번. 이닝 안에서 누가 이미 쳤는지가 이 순서로 갈린다.
  const order = (side: Side) => (list.indexOf(side) - start + list.length) % list.length;
  const mine = order(side);
  const now = order(state.turn);
  // 지금 차례보다 앞선 자리는 이번 이닝을 이미 쳤고, 뒤인 자리는 아직이다.
  return Math.max(0, mine <= now ? state.inning : state.inning - 1);
}

/** 한 게임의 평균 애버리지 — 당구장에서 실제로 보는 숫자. 자기가 친 이닝으로 나눈다. */
export function average(state: GameState, side: Side): number {
  const played = innings(state, side);
  if (played === 0) return 0;
  return state.players[side].score / played;
}

/**
 * 이닝별로 각자 몇 점을 쳤는지. 배열의 자리 하나가 이닝 하나다(0번 자리가 1이닝).
 *
 * 히스토리에서 되짚어 만든다. 득점 하나가 어느 이닝에 속하는지는 리듀서가 차례를 옮길
 * 때 쓰는 규칙과 똑같이 정한다 — 상대 차례에 넣은 점수는 그 사람의 새 차례에 속하고,
 * 그 차례가 선공에게 돌아온 것이면 이닝이 하나 올라간 뒤다. 여기서 규칙을 다시 쓰지
 * 않고 같은 조건을 그대로 두는 이유는, 둘이 어긋나는 순간 표와 점수가 서로 다른 말을
 * 하게 되기 때문이다.
 */
export function inningRuns(state: GameState): Record<Side, number[]> {
  const first = firstSide(state);
  const list = sides(state);
  const runs: Record<Side, number[]> = {};
  for (const side of list) runs[side] = [];

  for (const entry of state.history) {
    const moved = entry.delta > 0 && entry.side !== entry.turnBefore && entry.side === first;
    const inning = moved ? entry.inningBefore + 1 : entry.inningBefore;
    const at = Math.max(0, inning - 1);
    const list = runs[entry.side];
    while (list.length <= at) list.push(0);
    list[at] += entry.delta;
  }

  // 배열의 길이를 맞춰 둔다. 표가 이닝 수만큼 줄을 그릴 때 한쪽만 짧으면 곤란하다.
  const length = Math.max(0, ...list.map((side) => runs[side].length));
  for (const side of list) {
    while (runs[side].length < length) runs[side].push(0);
  }
  return runs;
}

/** 한 이닝에 몰아친 최고 점수 — 당구장에서 "하이런"이라고 부르는 그 숫자. */
export const highRun = (runs: number[] | undefined) =>
  runs && runs.length ? Math.max(0, ...runs) : 0;

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
  /** 자리 순서. 둘이 친 판에는 없다 — 그때는 순서가 하나뿐이었다. */
  order?: Side[];
  /**
   * 이닝별 득점. 히스토리 전체 대신 이것만 남긴다.
   *
   * 히스토리는 한 판에 수십 줄이고 그 안의 대부분은 되돌리기를 위한 것이라, 게임이
   * 끝나면 쓸 데가 없다. 반면 "몇 이닝에 몇 개를 쳤나"는 끝난 뒤에야 볼 수 있는 것이고
   * 숫자 스물몇 개면 담긴다 — 남길 값과 버릴 값이 여기서 갈린다.
   */
  runs?: Record<Side, number[]>;
  /**
   * 이 판에 남긴 노트.
   *
   * 기록에 함께 남는다 — 적어 둔 이유가 대개 나중에 다시 보려는 것이기 때문이다.
   * 빈 장은 저장하지 않으므로, 아무것도 적지 않은 판에는 이 값이 아예 없다.
   */
  notes?: NotePage[];
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
  order: state.order,
  runs: inningRuns(state),
  notes: keptNotes(state.notes),
});

/** 빈 장은 버린다. 열어만 보고 아무것도 안 한 판까지 노트가 있는 판으로 남을 이유는 없다. */
export function keptNotes(pages: NotePage[] | undefined): NotePage[] | undefined {
  const kept = (pages ?? []).filter(
    (page) =>
      (page.text ?? '').trim().length > 0 ||
      (page.strokes ?? []).length > 0 ||
      (page.balls ?? []).length > 0
  );
  return kept.length ? kept : undefined;
}
