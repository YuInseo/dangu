'use client';

import Link from 'graft/link';
import { useHardwareBack } from 'graft/native';
import { useRouter } from 'graft/navigation';
import { useEffect, useState } from 'react';

import {
  GAME_KINDS,
  SIDES,
  createGame,
  kindInfo,
  other,
  type GameKind,
  type GameState,
  type GameSummary,
  type Seat,
  type Side,
} from '../lib/game';
import { tap } from '../lib/platform';
import {
  DEFAULT_SETTINGS,
  clearCurrentGame,
  loadCurrentGame,
  loadHistory,
  loadSettings,
  saveCurrentGame,
  saveSettings,
  type AppSettings,
} from '../lib/storage';
import {
  averageOf,
  computeStats,
  humanDuration,
  recentOpponents,
  recentVenues,
  type OpponentCard,
  type Stats,
} from '../lib/stats';
import { useAccount } from '../lib/use-account';
import { SignIn } from './sign-in';

/**
 * 로비 — 게임이 시작되기 전에 정해야 하는 것들.
 *
 * 순서가 있다: 상대 이름 → 종목 → 핸디. 한 화면에 다 넣으면 폼이 되고, 폼은 당구장에서
 * 아무도 끝까지 채우지 않는다. 그래서 단계마다 큰 버튼 하나씩만 남기고, 각 단계는
 * 되돌아갈 수 있다.
 *
 * 이 화면이 하는 다른 하나는 "이어하기"다. 앱이 죽거나 폰이 잠겨도 진행 중이던 게임은
 * 기기에 저장되어 있으므로, 다시 켰을 때 제일 먼저 보이는 건 새 게임이 아니라 치던 게임이다.
 */

type Step = 'idle' | 'name' | 'kind' | 'handicap';

export function Lobby() {
  const router = useRouter();
  const account = useAccount();

  const [settings, setSettings] = useState<AppSettings>(DEFAULT_SETTINGS);
  const [resume, setResume] = useState<GameState | null>(null);
  const [ready, setReady] = useState(false);

  const [step, setStep] = useState<Step>('idle');
  /**
   * 몇 명이 치는지, 그리고 팀으로 치는지.
   *
   * 둘은 서로 다른 물음처럼 보이지만 한 줄에서 고른다 — 사람이 판을 차릴 때 하는 생각이
   * "둘이서? 셋이서? 아니면 편 갈라서?" 하나이기 때문이다.
   */
  const [count, setCount] = useState(2);
  const [team, setTeam] = useState(false);
  /** 상대들의 이름. 팀전이면 상대 팀 두 사람이다. */
  const [foes, setFoes] = useState<string[]>(['', '', '']);
  /** 팀전에서 내 편. */
  const [mate, setMate] = useState('');
  const [kind, setKind] = useState<GameKind>('four');
  /** 자리별 핸디. 0번이 나(또는 우리 팀)다. */
  const [targets, setTargets] = useState<number[]>([20, 20, 20, 20]);
  /** 선공 — 자리 번호. */
  const [firstAt, setFirstAt] = useState(0);
  const [lastCushion, setLastCushion] = useState(0);
  const [equalizer, setEqualizer] = useState(false);
  const [foul, setFoul] = useState(false);
  /** 요즘 친 사람들. 이름을 다시 적는 대신 고르는 자리다. */
  const [recent, setRecent] = useState<OpponentCard[]>([]);
  /** 첫 화면에 세울 숫자들. 기록 화면까지 가지 않고도 오늘이 어땠는지는 알아야 한다. */
  const [history, setHistory] = useState<GameSummary[]>([]);
  /** 어디서 치는지. 적어 두면 기록에 남고, 다음 판의 기본값이 된다. */
  const [venue, setVenue] = useState('');

  useEffect(() => {
    void (async () => {
      const [stored, current, history] = await Promise.all([
        loadSettings(),
        loadCurrentGame(),
        loadHistory(),
      ]);
      setRecent(recentOpponents(history));
      setHistory(history);
      setSettings(stored);
      setKind((stored.lastKind as GameKind) ?? 'four');
      setTargets([
        stored.lastTargets.white,
        stored.lastTargets.yellow,
        stored.lastTargets.yellow,
        stored.lastTargets.yellow,
      ]);
      setLastCushion(stored.lastCushion ?? 0);
      setEqualizer(stored.lastEqualizer ?? false);
      setFoul(stored.lastFoul ?? false);
      setVenue(stored.lastVenue ?? '');
      // 끝난 게임은 이어하기로 제안하지 않는다.
      setResume(current && !current.finishedAt ? current : null);
      setReady(true);
    })();
  }, []);

  /**
   * 하드웨어 뒤로가기는 단계를 되돌린다.
   *
   * 기본 동작은 앱을 닫는 것이라, 종목을 잘못 골라 뒤로 누른 사람이 로비가 아니라
   * 홈 화면으로 나가 버린다. 첫 단계에서만 원래대로 앱을 닫는다.
   */
  useHardwareBack(({ back }) => {
    const previous: Record<Step, Step | null> = { idle: null, name: 'idle', kind: 'name', handicap: 'kind' };
    const target = previous[step];
    if (target) setStep(target);
    else back();
  });

  const info = kindInfo(kind);
  const myName = settings.myName || '나';

  /**
   * 이 판에 앉을 자리들 — 치는 순서대로.
   *
   * 개인전이면 나부터, 그다음이 상대들이다. 팀전이면 자리가 둘뿐이고 각 자리가 두
   * 사람을 담는다: 팀은 점수를 함께 쓰므로 점수판에서 한 칸이고, 그 안에서 누가 칠
   * 차례인지는 점수판이 이닝으로 센다.
   */
  const seats: Seat[] = team
    ? [
        {
          name: `${myName} · ${mate || '파트너'}`,
          target: targets[0],
          members: [myName, mate || '파트너'],
        },
        {
          name: `${foes[0] || '상대 1'} · ${foes[1] || '상대 2'}`,
          target: targets[1],
          members: [foes[0] || '상대 1', foes[1] || '상대 2'],
        },
      ]
    : [
        { name: myName, target: targets[0] },
        ...Array.from({ length: count - 1 }, (_, index) => ({
          name: foes[index],
          target: targets[index + 1],
        })),
      ];

  const setTarget = (at: number, value: number) =>
    setTargets((current) => current.map((entry, index) => (index === at ? value : entry)));

  const setFoe = (at: number, value: string) =>
    setFoes((current) => current.map((entry, index) => (index === at ? value : entry)));

  // 첫 화면의 숫자들. 기록이 없으면 아무것도 세우지 않는다 — 빈 카드는 없느니만 못하다.
  const stats: Stats | null = history.length > 0 ? computeStats(history) : null;
  const lastGames = history.filter((game) => game.finishedAt).slice(0, 3);
  const places = recentVenues(history);

  const chooseKind = (next: GameKind) => {
    setKind(next);
    // 종목을 바꾸면 핸디의 출발점도 그 종목의 것으로 바뀐다 — 4구 20점에서 3구로 갔는데
    // 20점이 남아 있으면 그건 다른 게임이다.
    const target = kindInfo(next).defaultTarget;
    setTargets([target, target, target, target]);
    tap();
    setStep('handicap');
  };

  /**
   * 지난 상대를 고른다.
   *
   * 이름만 채우는 것이 아니라 그때의 판을 되살린다 — 같은 사람과는 대개 같은 종목을
   * 같은 핸디로 치기 때문이다. 그래서 이름·종목·핸디가 한 번에 정해지고, 남는 것은
   * 확인뿐이라 핸디 화면으로 바로 간다. 거기서 무엇이든 고칠 수 있다.
   */
  const pick = (card: OpponentCard) => {
    setCount(2);
    setTeam(false);
    setFoes([card.name, '', '']);
    setKind(card.last.kind);
    setTargets([card.last.mine, card.last.theirs, card.last.theirs, card.last.theirs]);
    setLastCushion(card.last.cushion);
    setFirstAt(0);
    tap();
    setStep('handicap');
  };

  const start = async () => {
    const game = createGame({
      kind,
      seats,
      first: SIDES[Math.min(firstAt, seats.length - 1)],
      me: 'white',
      // 쿠션 규칙은 4구의 관습이다. 다른 종목에 붙이면 화면에 뜻 없는 배지만 는다.
      lastCushion: kind === 'four' ? lastCushion : 0,
      equalizer,
      foul,
      venue,
    });
    await saveCurrentGame(game);
    await saveSettings({
      ...settings,
      lastKind: kind,
      lastTargets: { white: targets[0], yellow: targets[1] },
      lastCushion,
      lastEqualizer: equalizer,
      lastFoul: foul,
      lastVenue: venue.trim(),
    });
    tap('medium');
    router.push('/game');
  };

  if (!ready) return <div className="page" />;

  return (
    <div className="page">
      {resume && step === 'idle' && (
        <div className="card">
          <h2>치던 게임이 있습니다</h2>
          <p>
            {resume.players.white.name} {resume.players.white.score} : {resume.players.yellow.score}{' '}
            {resume.players.yellow.name} · {kindInfo(resume.kind).label}
          </p>
          <button className="primary" onClick={() => router.push('/game')}>
            이어하기
          </button>
          <button
            className="ghost"
            onClick={async () => {
              await clearCurrentGame();
              setResume(null);
            }}
          >
            버리고 새로 시작
          </button>
        </div>
      )}

      {step === 'idle' && (
        <>
          <button
            className="primary"
            onClick={() => {
              tap();
              setStep('name');
            }}
          >
            게임 시작
          </button>

          {/* 기록과 설정으로 가는 길은 아래 내비게이션이 갖고 있다. 같은 곳으로 가는
              문이 한 화면에 둘이면, 둘 중 어느 것이 진짜인지 매번 고르게 된다. */}

          {/*
            요즘 친 사람들.

            당구장에서 새 판을 차리는 일은 대개 "어제 그 사람과 한 판 더"다. 그런데
            앱은 매번 이름부터 다시 물었다 — 이미 아는 것을 묻는 자리였다. 여기서
            이름을 고르면 그때의 종목과 핸디까지 함께 살아난다.

            많이 친 순서가 아니라 최근 순이다. 통계에서 알고 싶은 것은 "누구와 많이
            쳤나"지만, 로비에서 필요한 것은 지금 앞에 있는 사람이고 그건 대개 어제 친
            사람이다.
          */}
          {recent.length > 0 && (
            <div className="card">
              <h2>다시 한 판</h2>
              <div className="faces">
                {recent.map((card) => (
                  <button key={card.name} className="face" onClick={() => pick(card)}>
                    <strong>{card.name}</strong>
                    <small>
                      {kindInfo(card.last.kind).label} {card.last.theirs}점 · {card.games}게임{' '}
                      {card.wins}승 {card.losses}패
                    </small>
                  </button>
                ))}
              </div>
            </div>
          )}

          {/*
            아무것도 친 적 없는 사람의 첫 화면.

            숫자로 채울 수 없는 자리다 — 0게임 0승 0패는 아무 말도 하지 않는다. 대신
            이 앱이 무엇을 하는 물건인지 세 줄로 적는다. 점수판의 기능은 대부분 화면
            어딘가를 눌러야 알 수 있는 것들이라(판을 아무 데나 누르면 1점이 오른다는
            것부터가 그렇다), 첫 판을 치기 전에 한 번은 말해 주는 편이 낫다.

            그리고 바로 칠 수 있는 버튼 하나. 이름을 묻지 않는다 — 처음 켠 사람이
            제일 하고 싶은 일은 앱이 어떻게 생겼는지 보는 것이지 폼을 채우는 게 아니고,
            이름은 판 위에서도 고칠 수 있다.
          */}
          {!stats && (
            <div className="card">
              <h2>처음 오셨네요</h2>
              <ul className="tips">
                <li>
                  <strong>자기 쪽 판을 아무 데나 누르면 1점</strong>
                  <span>큐를 든 채로 겨냥할 작은 버튼이 없습니다. 잘못 누르면 되돌리기.</span>
                </li>
                <li>
                  <strong>노트에 당구대를 그립니다</strong>
                  <span>공을 놓고 당점까지 찍어 둘 수 있고, 그 판의 기록에 함께 남습니다.</span>
                </li>
                <li>
                  <strong>남은 점수를 소리로 알려 줍니다</strong>
                  <span>“17점 남았습니다.” 화면을 보지 않아도 됩니다.</span>
                </li>
              </ul>
              <button
                className="secondary"
                onClick={async () => {
                  // 이름도 핸디도 묻지 않고 4구 20:20으로 바로. 판 위에서 다 고칠 수 있다.
                  setCount(2);
                  setTeam(false);
                  setFoes(['', '', '']);
                  setKind('four');
                  setTargets([20, 20, 20, 20]);
                  tap();
                  setStep('handicap');
                }}
              >
                이름 없이 바로 한 판
              </button>
            </div>
          )}

          {/*
            오늘.

            기록 화면까지 가지 않고도 알아야 하는 것이 하나 있다 — 오늘 몇 판 쳤고 몇 판
            이겼나. 당구장에서 이 앱을 여는 사람은 대개 그걸 궁금해하며 열고, 그때마다
            달력을 지나 아래로 내려가야 했다.

            오늘 친 판이 없으면 이번 달을 대신 적는다. "0게임 0승 0패"는 아무 말도 하지
            않는 카드이고, 그런 카드는 없느니만 못하다.
          */}
          {stats && (
            <div className="card">
              <h2>{stats.today.games > 0 ? '오늘' : '이번 달'}</h2>
              {(() => {
                const tally = stats.today.games > 0 ? stats.today : stats.month;
                return (
                  <>
                    <div className="glance">
                      <div>
                        <strong>{tally.games}</strong>
                        <small>게임</small>
                      </div>
                      <div>
                        <strong>{tally.wins}</strong>
                        <small>승</small>
                      </div>
                      <div>
                        <strong>{tally.losses}</strong>
                        <small>패</small>
                      </div>
                      <div>
                        <strong>{averageOf(tally).toFixed(3)}</strong>
                        <small>에버</small>
                      </div>
                    </div>
                    <p style={{ fontSize: '0.8rem' }}>
                      {humanDuration(tally.elapsedMs)} 쳤습니다
                      {stats.currentStreak > 1 ? ` · ${stats.currentStreak}연승 중` : ''}
                    </p>
                  </>
                );
              })()}

              {/* 최근 열 판의 승패. 숫자보다 이 점들이 요즘 어떤지를 먼저 말해 준다. */}
              {stats.recentForm.length > 0 && (
                <div className="form">
                  {[...stats.recentForm].reverse().map((result, index) => (
                    <span key={index} className={`dot ${result.toLowerCase()}`} aria-hidden="true" />
                  ))}
                  <span className="form-label">최근 {stats.recentForm.length}판</span>
                </div>
              )}
            </div>
          )}

          {/*
            마지막 판들.

            누구와 몇 대 몇이었는지 세 줄이면 어제 저녁이 떠오른다. 눌러서 기록으로
            가는 문이기도 하다 — 아래 내비게이션과 같은 곳이지만, 이쪽은 "그 판"을
            보러 가는 길이라 뜻이 다르다.
          */}
          {lastGames.length > 0 && (
            <div className="card">
              <h2>마지막 판</h2>
              <div className="last-games">
                {lastGames.map((game) => {
                  const me = game.me ?? 'white';
                  const mine = game.players[me];
                  const them = game.players[other(me)];
                  const won = game.winner === me;
                  const lost = game.winner === other(me);
                  return (
                    <button
                      key={game.id}
                      className="last-game"
                      onClick={() => {
                        tap();
                        router.push('/stats');
                      }}
                    >
                      <span className="who">{them?.name || '상대'}</span>
                      <span className={won ? 'score win' : lost ? 'score lose' : 'score'}>
                        {mine?.score ?? 0} : {them?.score ?? 0}
                      </span>
                      <span className="when">
                        {new Date(game.startedAt).toLocaleDateString('ko-KR', {
                          month: 'numeric',
                          day: 'numeric',
                        })}
                      </span>
                    </button>
                  );
                })}
              </div>
            </div>
          )}

          <div className="card">
            <h2>{account.account ? account.account.name : '계정'}</h2>
            {account.configured === false && (
              <p>
                Firebase가 설정되지 않아 기록은 이 기기에만 저장됩니다. 그래도 게임·핸디·통계는 전부
                동작합니다.
              </p>
            )}
            {account.configured === true && !account.account && <SignIn account={account} />}
            {/* 로그인한 사람에게는 이름만 남는다. 기록이 계정에 올라간다는 것은 로그인의
                뜻이지 새 소식이 아니고, 매번 읽을 문장도 아니다 — 자세한 것은 설정에 있다. */}
          </div>
        </>
      )}

      {step === 'name' && (
        <div className="card">
          <h2>누구와 치나요?</h2>

          {/*
            몇 명이 치는지.

            "둘이서? 셋이서? 아니면 편 갈라서?"는 판을 차릴 때 하는 한 가지 생각이라
            한 줄에 둔다. 팀은 2:2 하나뿐이다 — 3:3은 여섯 명이고, 그건 점수판 하나로
            돌아가는 판이 아니다.
          */}
          <div className="choices" style={{ gridTemplateColumns: 'repeat(4, 1fr)' }}>
            {[2, 3, 4].map((value) => (
              <button
                key={value}
                className="choice"
                aria-pressed={!team && count === value}
                onClick={() => {
                  setTeam(false);
                  setCount(value);
                  setFirstAt(0);
                  tap();
                }}
              >
                {value}명
              </button>
            ))}
            <button
              className="choice"
              aria-pressed={team}
              onClick={() => {
                setTeam(true);
                setCount(4);
                setFirstAt(0);
                tap();
              }}
            >
              2:2
            </button>
          </div>

          <div>
            <label className="label" htmlFor="me">
              내 이름
            </label>
            <input
              id="me"
              value={settings.myName}
              onChange={(event) => setSettings({ ...settings, myName: event.target.value })}
            />
          </div>

          {team && (
            <div>
              <label className="label" htmlFor="mate">
                내 편
              </label>
              <input
                id="mate"
                value={mate}
                placeholder="같은 팀 사람"
                onChange={(event) => setMate(event.target.value)}
              />
            </div>
          )}

          {Array.from({ length: team ? 2 : count - 1 }, (_, index) => (
            <div key={index}>
              <label className="label" htmlFor={`foe-${index}`}>
                {team ? `상대 팀 ${index + 1}` : count === 2 ? '상대 이름' : `상대 ${index + 1}`}
              </label>
              <input
                id={`foe-${index}`}
                autoFocus={index === 0 && !team}
                value={foes[index]}
                placeholder={index === 0 ? '예: 김프로' : '이름'}
                onChange={(event) => setFoe(index, event.target.value)}
                onKeyDown={(event) => {
                  if (event.key === 'Enter' && index === 0 && count === 2 && !team) setStep('kind');
                }}
              />
            </div>
          ))}

          {/* 지난 상대들. 여러 명이 치는 판에서는 첫 칸에 넣는다 — 대개 그 사람이 있고,
              나머지는 그날 처음 만난 사람인 경우가 많다. */}
          {recent.length > 0 && (
            <div className="chips">
              {recent.map((card) => (
                <button
                  key={card.name}
                  className={foes.includes(card.name) ? 'chip on' : 'chip'}
                  onClick={() => {
                    const at = foes.findIndex((name) => !name.trim());
                    setFoe(at === -1 ? 0 : at, card.name);
                    tap();
                  }}
                >
                  {card.name}
                </button>
              ))}
            </div>
          )}

          <div className="row">
            <button className="ghost" onClick={() => setStep('idle')}>
              취소
            </button>
            <button className="primary" onClick={() => setStep('kind')}>
              다음
            </button>
          </div>
        </div>
      )}

      {step === 'kind' && (
        <div className="card">
          <h2>종목</h2>
          <div className="choices">
            {GAME_KINDS.map((entry) => (
              <button
                key={entry.id}
                className="choice"
                aria-pressed={kind === entry.id}
                onClick={() => chooseKind(entry.id)}
              >
                {entry.label}
                <small>{entry.hint}</small>
              </button>
            ))}
          </div>
          <button className="ghost" onClick={() => setStep('name')}>
            ← 이름 다시
          </button>
        </div>
      )}

      {step === 'handicap' && (
        <div className="card">
          <h2>핸디 — 각자 몇 점?</h2>
          <p>
            {info.label} · 먼저 자기 점수에 닿는 사람이 이깁니다. 실력 차이는 여기서 맞춥니다.
          </p>

          {/* 자리마다 한 칸. 둘일 때는 나란히 둘, 셋 이상이면 두 줄로 접힌다. */}
          <div className="handicap">
            {seats.map((seat, index) => (
              <HandicapBox
                key={index}
                ball={index % 2 === 1 ? 'yellow' : 'white'}
                name={seat.name}
                value={targets[index]}
                onChange={(value) => setTarget(index, value)}
              />
            ))}
          </div>

          {kind === 'four' && (
            <div>
              <span className="label">마지막 쿠션</span>
              <div className="choices" style={{ gridTemplateColumns: 'repeat(4, 1fr)' }}>
                {[0, 1, 2, 3].map((value) => (
                  <button
                    key={value}
                    className="choice"
                    aria-pressed={lastCushion === value}
                    onClick={() => {
                      setLastCushion(value);
                      tap();
                    }}
                  >
                    {value === 0 ? '없음' : `${value}점`}
                  </button>
                ))}
              </div>
              <p style={{ fontSize: '0.78rem', color: 'rgba(243,244,246,0.55)', margin: '0.4rem 0 0' }}>
                {lastCushion === 0
                  ? '목표 점수에 닿으면 바로 끝납니다.'
                  : `목표 점수를 채운 다음, 쿠션으로 ${lastCushion}점을 더 쳐야 이깁니다 — ` +
                    `${targets[0]}점이면 실제로는 ${targets[0] + lastCushion}점입니다. 판정은 사람이 하고, 점수판은 쿠션 구간과 남은 점수를 보여 줍니다.`}
              </p>
            </div>
          )}

          <div>
            <span className="label">선공</span>
            <div className="choices" style={{ gridTemplateColumns: `repeat(${seats.length}, 1fr)` }}>
              {seats.map((seat, index) => (
                <button
                  key={index}
                  className="choice"
                  aria-pressed={firstAt === index}
                  onClick={() => {
                    setFirstAt(index);
                    tap();
                  }}
                >
                  {seat.name}
                </button>
              ))}
            </div>
          </div>

          {/* 후구는 선공이 정해져야 뜻이 생기는 규칙이라 그 바로 아래에 둔다. */}
          <div>
            <span className="label">후구</span>
            <div className="row">
              <button
                className="choice"
                aria-pressed={!equalizer}
                onClick={() => {
                  setEqualizer(false);
                  tap();
                }}
              >
                없음
              </button>
              <button
                className="choice"
                aria-pressed={equalizer}
                onClick={() => {
                  setEqualizer(true);
                  tap();
                }}
              >
                사용
              </button>
            </div>
            <p style={{ fontSize: '0.78rem', color: 'rgba(243,244,246,0.55)', margin: '0.4rem 0 0' }}>
              {equalizer
                ? `선공(${seats[Math.min(firstAt, seats.length - 1)]?.name ?? '선공'})이 먼저 목표를 채워도 ` +
                  '거기서 끝나지 않고 나머지가 한 차례씩 더 칩니다. 따라붙으면 매치포인트로 이어져, ' +
                  '한 이닝에서 더 친 쪽이 나올 때까지 계속됩니다.'
                : '선공이 목표를 채우는 순간 끝납니다.'}
            </p>
          </div>

          {/*
            뒷빡.

            누르는 자리의 뜻이 바뀌는 규칙이라 시작 전에 정해야 한다 — 켜져 있으면 상대
            판을 누르는 것이 "상대에게 한 점"이 아니라 "저 공을 맞혔다"가 된다. 판을 치는
            도중에 그 뜻이 바뀌면 이미 누른 것들이 무슨 뜻이었는지 알 수 없게 된다.
          */}
          <div>
            <span className="label">뒷빡</span>
            <div className="row">
              <button
                className="choice"
                aria-pressed={!foul}
                onClick={() => {
                  setFoul(false);
                  tap();
                }}
              >
                없음
              </button>
              <button
                className="choice"
                aria-pressed={foul}
                onClick={() => {
                  setFoul(true);
                  tap();
                }}
              >
                사용
              </button>
            </div>
            <p style={{ fontSize: '0.78rem', color: 'rgba(243,244,246,0.55)', margin: '0.4rem 0 0' }}>
              {foul
                ? '상대 판을 누르면 점수는 그대로 두고 차례만 넘어갑니다. 몇 점을 물릴지는 −1 버튼으로 ' +
                  '직접 빼세요 — 0점에서도 눌려서 −1, −2로 내려갑니다.'
                : '상대 판을 누르면 상대에게 한 점이 올라갑니다.'}
            </p>
          </div>

          {/*
            어디서 치는지.

            마지막에 두는 이유는 이것이 판을 정하는 값이 아니기 때문이다 — 종목이나 핸디를
            잘못 고르면 게임이 달라지지만, 장소는 적어도 안 적어도 게임은 똑같이 돌아간다.
            그래서 눈이 마지막에 닿는 자리, 시작 버튼 바로 위에 둔다.

            대개는 늘 가던 집이라 지난번 값이 그대로 채워져 있고, 다른 집에 간 날에만
            손이 간다.
          */}
          <div>
            <label className="label" htmlFor="venue">
              당구장 (선택)
            </label>
            <input
              id="venue"
              value={venue}
              placeholder="예: 대박당구장"
              onChange={(event) => setVenue(event.target.value)}
            />
            {places.length > 0 && (
              <div className="chips" style={{ marginTop: '0.4rem' }}>
                {places.map((name) => (
                  <button
                    key={name}
                    className={venue.trim() === name ? 'chip on' : 'chip'}
                    onClick={() => {
                      setVenue(venue.trim() === name ? '' : name);
                      tap();
                    }}
                  >
                    {name}
                  </button>
                ))}
              </div>
            )}
          </div>

          <button className="primary" onClick={() => void start()}>
            시작
          </button>
          <button className="ghost" onClick={() => setStep('kind')}>
            ← 종목 다시
          </button>
        </div>
      )}
    </div>
  );
}

/**
 * 핸디 하나를 정하는 상자.
 *
 * ±1, ±5 버튼과 직접 입력을 같이 둔다. 20점 근처는 버튼이 빠르고, 150점짜리 핸디는
 * 버튼으로 누르는 게 고문이라서.
 */
function HandicapBox({
  ball,
  name,
  value,
  onChange,
}: {
  /** 이 자리가 잡는 공. 셋째부터는 흰·노랑을 다시 쓴다 — 테이블의 수구가 둘뿐이라서. */
  ball: 'white' | 'yellow';
  name: string;
  value: number;
  onChange: (value: number) => void;
}) {
  const set = (next: number) => {
    onChange(Math.max(1, Math.min(999, next)));
    tap();
  };

  return (
    <div className={`box ${ball}`}>
      <strong style={{ fontSize: '0.9rem' }}>{name}</strong>
      <span className="value">{value}</span>
      <div className="row">
        <button onClick={() => set(value - 5)} aria-label="5점 빼기">
          −5
        </button>
        <button onClick={() => set(value - 1)} aria-label="1점 빼기">
          −1
        </button>
        <button onClick={() => set(value + 1)} aria-label="1점 더하기">
          +1
        </button>
        <button onClick={() => set(value + 5)} aria-label="5점 더하기">
          +5
        </button>
      </div>
      <input
        type="number"
        inputMode="numeric"
        value={value}
        aria-label={`${name} 목표 점수`}
        onChange={(event) => onChange(Math.max(1, Math.min(999, Number(event.target.value) || 1)))}
      />
    </div>
  );
}
