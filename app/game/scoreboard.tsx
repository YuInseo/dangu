'use client';

import { useHardwareBack } from 'graft/native';
import { useRouter } from 'graft/navigation';
import { useCallback, useEffect, useReducer, useRef, useState } from 'react';

import {
  average,
  cushionRemaining,
  displayScore,
  formatClock,
  kindInfo,
  needsCushion,
  other,
  reduce,
  remaining,
  summarize,
  type GameState,
  type Side,
} from '../../lib/game';
import { keepAwake, tap } from '../../lib/platform';
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
 * 화면의 절반씩이 각자의 것이고 — 왼쪽 흰 공, 오른쪽 노란 공 — 가운데 좁은 기둥에
 * 시계와 진행 버튼이 있다. 이 배치인 이유는 둘이 테이블을 사이에 두고 마주 서서
 * 각자 자기 쪽을 누르기 때문이다. 상대 점수를 올려 주려고 폰을 돌려 잡는 일이 없다.
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
    void (async () => {
      const summary = summarize(state);
      await recordGame(summary);
      // 클라우드는 사용자가 고른 경우에만. 로그인했다는 것과 클라우드에 저장하겠다는
      // 것은 다른 결정이고, 뒤엣것을 앞엣것으로 대신 정해 주지 않는다.
      if (account && (await cloudChosen())) await pushGame(account.uid, summary);
    })();
  }, [state?.finishedAt, saved, account]);

  // 게임 중 뒤로가기는 앱을 닫는 게 아니라 로비로 간다. 게임은 저장되어 있으므로
  // 로비의 "이어하기"로 그대로 돌아올 수 있다.
  useHardwareBack(() => router.push('/'));

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

  return (
    <>
      <div className="board">
        <PlayerSide side="white" state={state} onScore={score} onTurn={() => dispatch({ type: 'turn', side: 'white' })} />

        <div className="center">
          <div className="clock" aria-label="경과 시간">
            {formatClock(state.elapsedMs)}
          </div>
          <div className="inning">
            {info.label} · {state.inning}이닝
          </div>
          <div className="inning">
            에버 {average(state, 'white').toFixed(2)} / {average(state, 'yellow').toFixed(2)}
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
              <span style={{ fontSize: '0.7rem', opacity: 0.85 }}>
                {state.players[state.turn].name}
              </span>
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
        </div>

        <PlayerSide
          side="yellow"
          state={state}
          onScore={score}
          onTurn={() => dispatch({ type: 'turn', side: 'yellow' })}
        />
      </div>

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
            });
            await saveCurrentGame(next);
            dispatch(next);
            setSaved(false);
          }}
          onClose={async () => {
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
  onTurn,
}: {
  side: Side;
  state: GameState;
  onScore: (side: Side, delta: number) => void;
  onTurn: () => void;
}) {
  const player = state.players[side];
  const [manual, setManual] = useState('');
  const active = state.turn === side;
  const cushion = needsCushion(state, side);

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
        className="tap"
        onClick={() => onScore(side, 1)}
        aria-label={`${player.name} 1점 더하기`}
      >
        {/* 목표 점수를 채우면 숫자가 0으로 돌아가고 쿠션 점수를 센다. 그래서 지금
            보이는 숫자가 무엇인지 위에 적어 준다 — 이 한 줄이 없으면 0으로 돌아간
            점수판이 오히려 더 헷갈린다. */}
        {cushion && <div className="cushion">쿠션</div>}

        <div className={cushion ? 'score cushion-score' : 'score'} aria-live="polite">
          {displayScore(state, side)}
        </div>

        <div className="target">
          {cushion
            ? `쿠션 ${state.lastCushion}점 중 ${cushionRemaining(state, side)}점 남음`
            : `${player.target}점${state.lastCushion > 0 ? ` +쿠션 ${state.lastCushion}` : ''} · ${remaining(state, side)}점 남음`}
        </div>

        <div className="spacer" />

        <div className="tap-hint">눌러서 +1</div>
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
            disabled={player.score === 0}
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

/* 결과 --------------------------------------------------------------- */

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
        <button className="primary" onClick={() => void onAgain()}>
          한 판 더
        </button>
        <button className="secondary" onClick={() => void onClose()}>
          기록 보기
        </button>
      </div>
    </div>
  );
}
