'use client';

import { useHardwareBack } from 'graft/native';
import { useRouter } from 'graft/navigation';
import { useCallback, useEffect, useReducer, useRef, useState } from 'react';

import {
  average,
  cushionRemaining,
  displayScore,
  formatClock,
  inDecider,
  innings,
  kindInfo,
  needsCushion,
  other,
  reduce,
  remaining,
  summarize,
  readNotes,
  turnElapsed,
  type GameState,
  type NotePage,
  type Side,
} from '../../lib/game';
import { keepAwake, tap } from '../../lib/platform';
import { Notes } from './notes';
import {
  clearCurrentGame,
  cloudChosen,
  loadCurrentGame,
  loadSettings,
  recordGame,
  saveCurrentGame,
} from '../../lib/storage';
import { pushGame } from '../../lib/firebase';
import { useAccount } from '../../lib/use-account';

/**
 * 점수판.
 *
 * 화면의 절반씩이 각자의 것이고 — 왼쪽 흰 공, 오른쪽 노란 공 — 시계와 진행 버튼은
 * 아래 푸터 한 줄에 있다. 이 배치인 이유는 둘이 테이블을 사이에 두고 마주 서서 각자
 * 자기 쪽을 누르기 때문이다. 상대 점수를 올려 주려고 폰을 돌려 잡는 일이 없다.
 *
 * 상태는 `lib/game.ts`의 리듀서가 전부 가지고 있고, 여기서는 그리기와 저장만 한다.
 * 저장은 상태가 바뀔 때마다 — 앱이 언제 죽을지 모르는 게 모바일이고, 점수판이 마지막
 * 한 점을 잃는 건 게임을 다시 치는 것과 같다.
 */
export function Scoreboard() {
  const router = useRouter();
  const { account } = useAccount();
  const [state, dispatch] = useReducer(reduce, null as unknown as GameState);
  const [missing, setMissing] = useState(false);
  const [saved, setSaved] = useState(false);
  const started = useRef(false);
  /** 진행 중인 저장. 점수판을 떠나기 전에 이게 끝나기를 기다린다. */
  const storing = useRef<Promise<void> | null>(null);
  /** 노트를 열어 두었는지. 게임은 그대로 돌아간다 — 시계도, 저장도. */
  const [noting, setNoting] = useState(false);
  /** 한 차례에 주는 시간(ms). 0이면 샷 클락을 쓰지 않는다. */
  const [limit, setLimit] = useState(0);
  /** 시간이 다 됐다고 울린 차례. 같은 차례에 두 번 울리지 않으려고 기억한다. */
  const buzzed = useRef<number | null>(null);

  /* 불러오기 ------------------------------------------------------- */

  useEffect(() => {
    void (async () => {
      const current = await loadCurrentGame();
      if (!current) {
        setMissing(true);
        return;
      }
      dispatch(current);
      started.current = true;
    })();
  }, []);

  /* 저장 ----------------------------------------------------------- */

  useEffect(() => {
    if (!state || !started.current) return;
    void saveCurrentGame(state);
  }, [state]);

  /* 시계 ----------------------------------------------------------- */

  useEffect(() => {
    if (!state?.running || state.finishedAt) return;
    // 1초마다 흐른 시간을 더한다. `Date.now()` 차이로 계산하지 않는 이유는 일시정지
    // 때문이다 — 멈춘 동안의 벽시계 시간은 이 게임의 시간이 아니다.
    const timer = setInterval(() => dispatch({ type: 'tick', ms: 1000 }), 1000);
    return () => clearInterval(timer);
  }, [state?.running, state?.finishedAt]);

  /* 샷 클락 ---------------------------------------------------------- */

  useEffect(() => {
    void loadSettings().then((settings) =>
      setLimit(Math.max(0, Math.round(settings.turnSeconds ?? 0)) * 1000)
    );
  }, []);

  // 시간을 다 쓰면 한 번 울린다. 큐를 들고 테이블을 보고 있는 사람에게 화면의 색만으로는
  // 닿지 않는다. `turnAt`으로 기억해 두어 한 차례에 한 번만 울린다.
  useEffect(() => {
    if (!state || !limit || state.finishedAt || !state.running) return;
    if (turnElapsed(state) < limit) return;
    if (buzzed.current === state.turnAt) return;
    buzzed.current = state.turnAt;
    tap('heavy');
  }, [state?.elapsedMs, state?.turnAt, state?.running, state?.finishedAt, limit]);

  /* 화면 꺼짐 방지 -------------------------------------------------- */

  useEffect(() => {
    if (!state || state.finishedAt) return;
    let release: (() => void) | undefined;
    void (async () => {
      const settings = await loadSettings();
      if (settings.keepAwake) release = await keepAwake(true);
    })();
    return () => release?.();
  }, [state?.finishedAt]);

  /* 끝난 게임 저장 -------------------------------------------------- */

  useEffect(() => {
    if (!state?.finishedAt || saved) return;
    setSaved(true);
    // 저장이 끝나는 시점을 붙잡아 둔다. "게임 종료"가 기록 화면으로 넘어가기 전에 이걸
    // 기다린다 — 그러지 않으면 방금 친 판이 아직 없는 목록이 뜬다.
    storing.current = (async () => {
      const summary = summarize(state);
      await recordGame(summary);
      // 클라우드는 사용자가 고른 경우에만. 로그인했다는 것과 클라우드에 저장하겠다는
      // 것은 다른 결정이고, 뒤엣것을 앞엣것으로 대신 정해 주지 않는다.
      if (account && (await cloudChosen())) await pushGame(account.uid, summary);
    })();
    void storing.current;
  }, [state?.finishedAt, saved, account]);

  // 게임 중 뒤로가기는 앱을 닫는 게 아니라 로비로 간다. 게임은 저장되어 있으므로
  // 로비의 "이어하기"로 그대로 돌아올 수 있다. 다만 노트가 열려 있으면 그것부터
  // 닫는다 — 안드로이드에서 뒤로가기는 "지금 위에 뜬 것"을 닫는 버튼이다.
  useHardwareBack(() => {
    if (noting) setNoting(false);
    else router.push('/');
  });

  const score = useCallback(
    (side: Side, delta: number) => {
      dispatch({ type: 'score', side, delta });
      tap(delta > 0 ? 'medium' : 'light');
    },
    [dispatch]
  );


  if (missing) {
    return (
      <div className="page">
        <div className="card">
          <h2>진행 중인 게임이 없습니다</h2>
          <p>로비에서 새 게임을 시작하세요.</p>
          <button className="primary" onClick={() => router.push('/')}>
            로비로
          </button>
        </div>
      </div>
    );
  }

  if (!state) return <div className="page" />;

  const info = kindInfo(state.kind);

  // 뒷빡. 치던 사람이 상대 수구를 맞혔다는 뜻이라, 어느 판을 눌렀든 기록은 *치던 쪽*에
  // 남는다 — 상대 판을 누르는 것은 "네 공을 맞혔다"는 신고이지 상대의 득점이 아니다.
  const foul = () => {
    dispatch({ type: 'foul', side: state.turn });
    tap('heavy');
  };

  return (
    <>
      <div className="board">
        <PlayerSide
          side="white"
          state={state}
          onScore={score}
          onFoul={foul}
          onTurn={() => dispatch({ type: 'turn', side: 'white' })}
        />

        <PlayerSide
          side="yellow"
          state={state}
          onScore={score}
          onFoul={foul}
          onTurn={() => dispatch({ type: 'turn', side: 'yellow' })}
        />
      </div>

      {/*
        진행 버튼은 화면 아래 한 줄에 모여 있다.

        가운데 기둥이 하던 일인데, 그러느라 두 사람의 판이 각각 화면의 3분의 1로 줄었다.
        점수는 방 건너편에서도 읽혀야 하는 이 앱에서 그건 비싼 값이다. 아래로 내리면
        판은 절반씩을 온전히 가져가고, 버튼은 손이 원래 가는 자리로 온다.
      */}
      <footer className="footer">
        {/*
          샷 클락. 지금 치는 사람에게 남은 시간이다.

          숫자가 아니라 막대인 이유는 이걸 보는 사람이 큐를 들고 테이블 위를 보고 있기
          때문이다. 곁눈질로 알 수 있어야 하고, 그때 읽히는 것은 자리와 색이지 숫자가
          아니다. 초록에서 노랑을 지나 빨강으로 가는 동안 남은 시간이 줄어든다.
        */}
        {limit > 0 && !state.finishedAt && <ShotClock spent={turnElapsed(state)} limit={limit} />}

        <div className="meta">
          <span className="clock" aria-label="경과 시간">
            {formatClock(state.elapsedMs)}
          </span>
          {/* 에버와 이닝은 각자의 판으로 옮겼다 — 사람마다 다른 숫자라 한 줄에 몰아
              적으면 어느 쪽이 자기 것인지 매번 읽어야 한다. */}
          <span>{info.label}</span>

          {/*
            노트는 여기 있다.
            아래 네 버튼은 치는 동안 손이 가는 자리라 다섯 번째를 끼워 넣으면 넷이 다
            좁아진다. 노트는 한 판에 몇 번 열지 않는 것이므로 시계 줄이 맞다.
          */}
          <button className="note-open" onClick={() => setNoting(true)}>
            노트{state.notes?.length ? ` ${state.notes.length}` : ''}
          </button>

          {/* 여기서부터는 이닝이 끝날 때마다 승부가 갈릴 수 있다. 치는 사람이 그걸
              모르고 있으면 안 되므로 크게 말해 준다. */}
          {inDecider(state) && (
            <span className="last-turn">
              후구 · 이 이닝을 더 친 쪽이 이깁니다
            </span>
          )}
        </div>

        <div className="stack">
          <button
            className="turn"
            onClick={() => {
              dispatch({ type: 'turn' });
              tap();
            }}
          >
            턴 넘김
            <br />
            <span className="sub">{state.players[state.turn].name}</span>
          </button>

          <button onClick={() => dispatch({ type: 'undo' })} disabled={state.history.length === 0}>
            되돌리기
          </button>

          <button onClick={() => dispatch({ type: state.running ? 'pause' : 'resume' })}>
            {state.running ? '일시정지' : '재개'}
          </button>

          <button className="finish" onClick={() => dispatch({ type: 'finish' })}>
            종료
          </button>
        </div>
      </footer>

      {noting && (
        <Notes
          pages={readNotes(state.notes)}
          onChange={(pages: NotePage[]) => dispatch({ type: 'notes', pages })}
          onClose={() => setNoting(false)}
        />
      )}

      {state.finishedAt && (
        <ResultSheet
          state={state}
          onAgain={async () => {
            // 같은 사람, 같은 종목, 같은 핸디로 한 판 더 — 당구장에서 제일 흔한 다음 행동이다.
            const { createGame } = await import('../../lib/game');
            const next = createGame({
              kind: state.kind,
              white: { name: state.players.white.name, target: state.players.white.target },
              yellow: { name: state.players.yellow.name, target: state.players.yellow.target },
              // 진 사람이 선공. 이것도 당구장 관습이다.
              first: state.winner ? other(state.winner) : 'white',
              me: state.me,
              lastCushion: state.lastCushion,
              equalizer: state.equalizer,
            });
            await saveCurrentGame(next);
            dispatch(next);
            setSaved(false);
          }}
          onClose={async () => {
            // 기록은 게임이 끝난 순간 이미 저장됐다. 여기서는 그게 끝나기를 기다렸다가
            // 진행 중인 게임을 비우고 기록으로 넘긴다.
            await storing.current;
            await clearCurrentGame();
            router.push('/stats');
          }}
        />
      )}
    </>
  );
}

/* 한쪽 --------------------------------------------------------------- */

function PlayerSide({
  side,
  state,
  onScore,
  onFoul,
  onTurn,
}: {
  side: Side;
  state: GameState;
  onScore: (side: Side, delta: number) => void;
  onFoul: () => void;
  onTurn: () => void;
}) {
  const player = state.players[side];
  const [manual, setManual] = useState('');
  const active = state.turn === side;
  const cushion = needsCushion(state, side);

  /*
    뒷빡을 쓰는 판에서 상대 판을 누르는 일.

    상대 수구를 맞혔을 때 누른다. 그런데 이건 상대가 한 점을 얻은 것이 아니다 — 친
    사람의 실수이므로 그 사람 쪽에 −1이 쌓이고 차례가 넘어간다. 그래서 지금 치고 있지
    않은 쪽의 넓은 자리는 +1이 아니라 "뒷빡"이 된다. 자기 차례인 쪽은 그대로 +1이다.

    뒷빡을 안 쓰기로 한 판에서는 이 자리가 예전처럼 +1로 남는다. 두 사람의 점수를 한
    사람이 몰아서 넣는 일이 흔하기 때문이다.
  */
  const foulTap = state.foul === true && !active && !state.finishedAt;

  return (
    <section
      className={`side ${side}${active ? ' active' : ''}${cushion ? ' in-cushion' : ''}`}
      aria-label={`${player.name} 점수판`}
    >
      <button className="name" onClick={onTurn} title="이 사람 차례로">
        {player.name}
      </button>

      {/*
        자기 쪽의 빈 곳은 전부 +1 버튼이다.

        큐를 들고 한 손으로 누르는 화면에서 제일 흔한 동작이 "한 점 올리기"인데, 그걸
        아래쪽 작은 버튼을 겨냥해야만 할 수 있게 두면 손이 많이 간다. 자기 색 판을
        아무 데나 치면 1점 — 잘못 눌러도 가운데 되돌리기가 그대로 받는다.
      */}
      <button
        className={foulTap ? 'tap foul-tap' : 'tap'}
        onClick={() => (foulTap ? onFoul() : onScore(side, 1))}
        aria-label={
          foulTap
            ? `${state.players[state.turn].name} 뒷빡, 차례 넘기기`
            : `${player.name} 1점 더하기`
        }
      >
        {/*
          목표 점수를 채우면 숫자가 0으로 돌아가고 쿠션 점수를 센다. 그래서 지금 보이는
          숫자가 무엇인지 위에 적어 준다 — 이 한 줄이 없으면 0으로 돌아간 점수판이
          오히려 더 헷갈린다.

          쿠션 구간이 아닐 때도 자리는 남겨 둔다. 한쪽에만 이 딱지가 붙으면 그쪽 점수만
          아래로 밀려서, 마주 보는 두 숫자의 높이가 어긋난다. 점수판에서 두 숫자는
          같은 줄에 있어야 한눈에 비교된다.
        */}
        <div className="cushion" data-empty={!cushion} aria-hidden={!cushion}>
          쿠션
        </div>

        <div className={cushion ? 'score cushion-score' : 'score'} aria-live="polite">
          {/* 뒷빡으로 0 아래에 간 점수. 빼기표는 이 앱의 다른 숫자들과 같은 −(U+2212)를
              쓴다 — 하이픈은 이 크기에서 눈에 띄게 짧고 위치도 어긋난다. */}
          {displayScore(state, side) < 0
            ? `−${Math.abs(displayScore(state, side))}`
            : displayScore(state, side)}
        </div>

        <div className="target">
          {cushion
            ? `쿠션 ${state.lastCushion}점 중 ${cushionRemaining(state, side)}점 남음`
            : `${player.target}점${state.lastCushion > 0 ? ` +쿠션 ${state.lastCushion}` : ''} · ${remaining(state, side)}점 남음`}
        </div>

        {/* 에버는 자기가 친 이닝으로 나눈 값이라 사람마다 다르다. 그래서 가운데 한 줄에
            둘을 몰아 적는 대신 각자의 판에 적는다 — 자기 숫자를 자기 쪽에서 본다. */}
        <div className="rate">
          에버 {average(state, side).toFixed(2).replace('-', '−')} · {innings(state, side)}이닝
          {/* 뒷빡은 점수에서 깎지 않고 따로 센다. 점수판의 큰 숫자는 "몇 개를 쳤나"이고,
              뒷빡은 "몇 번 실수했나"라 같은 자리에 섞으면 둘 다 못 읽는다. */}
          {state.foul === true && (
            <>
              {' · '}
              <span className="fouls" data-zero={!player.fouls}>
                뒷빡 {player.fouls ? `−${player.fouls}` : 0}
              </span>
            </>
          )}
        </div>

        <div className="spacer" />

        {/* 뒷빡 자리에는 안내를 적지 않는다. 이 판을 누르는 사람은 방금 무슨 일이
            있었는지 이미 알고 있고, 남는 것은 에버 줄의 −N 하나면 된다. 글자는 지우되
            자리는 남긴다 — 한쪽만 없어지면 두 판의 숫자 높이가 어긋난다. */}
        <div className="tap-hint" data-empty={foulTap}>
          눌러서 +1
        </div>
      </button>

      {/* 큰 것이 더하기다. 당구에서 점수는 거의 언제나 올라가고, 빼기는 잘못 눌렀을
          때만 쓴다 — 크기 차이가 그 빈도 차이를 말한다. */}
      <div className="pad">
        {[1, 2, 3].map((delta) => (
          <button key={delta} className="plus" onClick={() => onScore(side, delta)}>
            +{delta}
          </button>
        ))}
      </div>

      <div className="pad">
        {[1, 2, 3].map((delta) => (
          <button
            key={delta}
            className="minus"
            onClick={() => onScore(side, -delta)}
            // 뒷빡으로 0 아래에 있는 점수는 손으로 더 못 내린다. 손으로 누르는 −1은
            // 잘못 올린 것을 되돌리는 버튼이고, 되돌릴 것이 없으면 할 일이 없다.
            disabled={player.score <= 0}
          >
            −{delta}
          </button>
        ))}
      </div>

      {/* 3점을 넘겨 한 번에 올릴 때. 포켓볼에서 한 큐에 여러 개를 넣거나, 점수를
          잘못 세어 통째로 고칠 때 쓴다. */}
      <form
        className="manual"
        onSubmit={(event) => {
          event.preventDefault();
          const value = Number(manual);
          if (!Number.isFinite(value) || value === 0) return;
          onScore(side, value);
          setManual('');
        }}
      >
        <input
          type="number"
          inputMode="numeric"
          value={manual}
          placeholder="직접"
          aria-label={`${player.name} 점수 직접 입력`}
          onChange={(event) => setManual(event.target.value)}
        />
        <button type="submit">더하기</button>
      </form>
    </section>
  );
}

/* 샷 클락 ------------------------------------------------------------- */

/**
 * 지금 차례에 남은 시간을 색과 길이로 보여 주는 막대.
 *
 * 초록 → 노랑 → 빨강. 경계를 60%와 85%에 둔 것은, 노랑이 "슬슬 쳐야 한다"가 아니라
 * "이제 정말 쳐야 한다"의 자리에 있어야 하기 때문이다. 너무 일찍 노래지면 아무도 안
 * 본다. 시간을 다 쓰면 막대가 가득 찬 빨강으로 남고 남은 초는 0으로 멈춘다 — 넘긴
 * 시간을 음수로 세는 것은 점수판이 할 일이 아니고, 벌점은 사람이 정한다.
 */
function ShotClock({ spent, limit }: { spent: number; limit: number }) {
  const ratio = Math.min(1, spent / limit);
  const left = Math.max(0, Math.ceil((limit - spent) / 1000));
  const stage = ratio >= 0.85 ? 'over' : ratio >= 0.6 ? 'warn' : 'fine';

  return (
    <div
      className={`shot ${stage}`}
      role="timer"
      aria-label={`이번 차례 남은 시간 ${left}초`}
    >
      <div className="bar">
        <div className="fill" style={{ width: `${ratio * 100}%` }} />
      </div>
      <span className="left">{left}</span>
    </div>
  );
}

/* 결과 --------------------------------------------------------------- */

/**
 * 끝난 판의 결과.
 *
 * 다음에 할 일은 둘 중 하나다 — 한 판 더 치거나, 오늘은 여기까지거나. 그래서 버튼도
 * 둘이다. 기록은 이 시트가 뜨는 순간 이미 저장되어 있으므로 "저장"은 물어볼 일이
 * 아니고, "게임 종료"가 그 저장된 기록으로 데려다준다.
 */
function ResultSheet({
  state,
  onAgain,
  onClose,
}: {
  state: GameState;
  onAgain: () => Promise<void>;
  onClose: () => Promise<void>;
}) {
  const winner = state.winner ? state.players[state.winner] : null;

  return (
    <div className="sheet" role="dialog" aria-modal="true">
      <div className="inner">
        <div className="winner">{winner ? `${winner.name} 승리` : '무승부'}</div>
        <p className="card" style={{ background: 'transparent', border: 0, padding: 0 }}>
          {state.players.white.name} {state.players.white.score} : {state.players.yellow.score}{' '}
          {state.players.yellow.name}
          <br />
          {kindInfo(state.kind).label} · {state.inning}이닝 · {formatClock(state.elapsedMs)}
        </p>
        {/* 저장은 이미 끝났다. 물어보지 않고 알려만 준다. */}
        <p className="saved">기록에 저장했습니다</p>
        <button className="primary" onClick={() => void onAgain()}>
          한 판 더
        </button>
        <button className="secondary" onClick={() => void onClose()}>
          게임 종료
        </button>
      </div>
    </div>
  );
}
