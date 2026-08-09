'use client';

import { useHardwareBack } from 'graft/native';
import { useRouter } from 'graft/navigation';
import { useCallback, useEffect, useReducer, useRef, useState } from 'react';

import {
  average,
  cushionRemaining,
  displayScore,
  formatClock,
  innings,
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
  removeGame,
  saveCurrentGame,
} from '../../lib/storage';
import { deleteGame, pushGame } from '../../lib/firebase';
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
  /** 이 판을 기록에서 빼기로 했는지. 결과 화면의 "기록 안 함"이 정한다. */
  const [discarded, setDiscarded] = useState(false);
  const started = useRef(false);
  /** 진행 중인 저장. 지우기 전에 이게 끝나기를 기다린다. */
  const storing = useRef<Promise<void> | null>(null);

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
    // 저장이 끝나는 시점을 붙잡아 둔다. "기록 안 함"이 이것보다 먼저 끝나면 방금 지운
    // 자리에 저장이 뒤늦게 도착해, 지웠는데 남아 있는 기록이 된다.
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

        <PlayerSide
          side="yellow"
          state={state}
          onScore={score}
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
        <div className="meta">
          <span className="clock" aria-label="경과 시간">
            {formatClock(state.elapsedMs)}
          </span>
          {/* 에버와 이닝은 각자의 판으로 옮겼다 — 사람마다 다른 숫자라 한 줄에 몰아
              적으면 어느 쪽이 자기 것인지 매번 읽어야 한다. */}
          <span>{info.label}</span>
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
            setDiscarded(false);
          }}
          onClose={async () => {
            await clearCurrentGame();
            router.push('/stats');
          }}
          discarded={discarded}
          onDiscard={async () => {
            // 저장이 끝난 뒤에 지운다. 순서가 반대면 지운 자리에 저장이 뒤늦게 도착한다.
            await storing.current;
            await removeGame(state.id);
            if (account) await deleteGame(account.uid, state.id);
            setDiscarded(true);
            tap();
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
          {displayScore(state, side)}
        </div>

        <div className="target">
          {cushion
            ? `쿠션 ${state.lastCushion}점 중 ${cushionRemaining(state, side)}점 남음`
            : `${player.target}점${state.lastCushion > 0 ? ` +쿠션 ${state.lastCushion}` : ''} · ${remaining(state, side)}점 남음`}
        </div>

        {/* 에버는 자기가 친 이닝으로 나눈 값이라 사람마다 다르다. 그래서 가운데 한 줄에
            둘을 몰아 적는 대신 각자의 판에 적는다 — 자기 숫자를 자기 쪽에서 본다. */}
        <div className="rate">
          에버 {average(state, side).toFixed(2)} · {innings(state, side)}이닝
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
  onDiscard,
  discarded,
}: {
  state: GameState;
  onAgain: () => Promise<void>;
  onClose: () => Promise<void>;
  onDiscard: () => Promise<void>;
  discarded: boolean;
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

        {/*
          연습으로 친 판, 장난으로 눌러 끝난 판, 남이 잠깐 잡고 친 판.
          끝난 게임은 이미 저장된 뒤이므로 이 버튼이 하는 일은 그걸 도로 빼는 것이다.
          승률과 에버는 한 번 틀어지면 조용히 틀린 채로 남는 숫자라 빼는 길이 있어야 한다.

          누른 뒤에도 시트는 그대로 둔다 — 기록에서 뺐다고 해서 "한 판 더"가 필요 없어지는
          것은 아니고, 오히려 연습 판이었다면 더 그렇다.
        */}
        {discarded ? (
          <p className="notice">이 판은 기록하지 않았습니다.</p>
        ) : (
          <button className="ghost" onClick={() => void onDiscard()}>
            기록 안 함
          </button>
        )}
      </div>
    </div>
  );
}
