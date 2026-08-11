'use client';

import Link from 'graft/link';
import { useHardwareBack } from 'graft/native';
import { useRouter } from 'graft/navigation';
import { useEffect, useState } from 'react';

import { GAME_KINDS, createGame, kindInfo, type GameKind, type GameState, type Side } from '../lib/game';
import { tap } from '../lib/platform';
import {
  DEFAULT_SETTINGS,
  clearCurrentGame,
  loadCurrentGame,
  loadSettings,
  saveCurrentGame,
  saveSettings,
  type AppSettings,
} from '../lib/storage';
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
  const [opponent, setOpponent] = useState('');
  const [kind, setKind] = useState<GameKind>('four');
  const [targets, setTargets] = useState({ white: 20, yellow: 20 });
  const [first, setFirst] = useState<Side>('white');
  const [lastCushion, setLastCushion] = useState(0);
  const [equalizer, setEqualizer] = useState(false);
  const [foul, setFoul] = useState(false);

  useEffect(() => {
    void (async () => {
      const [stored, current] = await Promise.all([loadSettings(), loadCurrentGame()]);
      setSettings(stored);
      setKind((stored.lastKind as GameKind) ?? 'four');
      setTargets(stored.lastTargets);
      setLastCushion(stored.lastCushion ?? 0);
      setEqualizer(stored.lastEqualizer ?? false);
      setFoul(stored.lastFoul ?? false);
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

  const chooseKind = (next: GameKind) => {
    setKind(next);
    // 종목을 바꾸면 핸디의 출발점도 그 종목의 것으로 바뀐다 — 4구 20점에서 3구로 갔는데
    // 20점이 남아 있으면 그건 다른 게임이다.
    const target = kindInfo(next).defaultTarget;
    setTargets({ white: target, yellow: target });
    tap();
    setStep('handicap');
  };

  const start = async () => {
    const game = createGame({
      kind,
      white: { name: settings.myName || '나', target: targets.white },
      yellow: { name: opponent, target: targets.yellow },
      first,
      me: 'white',
      // 쿠션 규칙은 4구의 관습이다. 다른 종목에 붙이면 화면에 뜻 없는 배지만 는다.
      lastCushion: kind === 'four' ? lastCushion : 0,
      equalizer,
      foul,
    });
    await saveCurrentGame(game);
    await saveSettings({
      ...settings,
      lastKind: kind,
      lastTargets: targets,
      lastCushion,
      lastEqualizer: equalizer,
      lastFoul: foul,
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

          <div className="card">
            <h2>{account.account ? account.account.name : '계정'}</h2>
            {account.configured === false && (
              <p>
                Firebase가 설정되지 않아 기록은 이 기기에만 저장됩니다. 그래도 게임·핸디·통계는 전부
                동작합니다.
              </p>
            )}
            {account.configured === true && !account.account && <SignIn account={account} />}
            {account.account && <p>기록이 계정에 저장되고 있습니다.</p>}
          </div>
        </>
      )}

      {step === 'name' && (
        <div className="card">
          <h2>누구와 치나요?</h2>
          <div>
            <label className="label" htmlFor="opponent">
              상대 이름
            </label>
            <input
              id="opponent"
              autoFocus
              value={opponent}
              placeholder="예: 김프로"
              onChange={(event) => setOpponent(event.target.value)}
              onKeyDown={(event) => {
                if (event.key === 'Enter') setStep('kind');
              }}
            />
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

          <div className="handicap">
            <HandicapBox
              side="white"
              name={settings.myName || '나'}
              value={targets.white}
              onChange={(value) => setTargets((current) => ({ ...current, white: value }))}
            />
            <HandicapBox
              side="yellow"
              name={opponent || '상대'}
              value={targets.yellow}
              onChange={(value) => setTargets((current) => ({ ...current, yellow: value }))}
            />
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
                    `${targets.white}점이면 실제로는 ${targets.white + lastCushion}점입니다. 판정은 사람이 하고, 점수판은 쿠션 구간과 남은 점수를 보여 줍니다.`}
              </p>
            </div>
          )}

          <div>
            <span className="label">선공</span>
            <div className="row">
              <button
                className="choice"
                aria-pressed={first === 'white'}
                onClick={() => setFirst('white')}
              >
                {settings.myName || '나'} (흰 공)
              </button>
              <button
                className="choice"
                aria-pressed={first === 'yellow'}
                onClick={() => setFirst('yellow')}
              >
                {opponent || '상대'} (노란 공)
              </button>
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
                ? `선공(${first === 'white' ? settings.myName || '나' : opponent || '상대'})이 먼저 목표를 채워도 ` +
                  '거기서 끝나지 않고 후공이 한 차례를 더 칩니다. 후공이 따라붙으면 매치포인트로 이어져, ' +
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
                ? '치는 사람이 상대 수구를 맞히면 상대 판을 누릅니다. 상대에게 점수가 가는 게 아니라 ' +
                  '내 점수가 1점 깎이고(0점이면 −1점) 차례가 넘어갑니다.'
                : '상대 판을 누르면 상대에게 한 점이 올라갑니다.'}
            </p>
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
  side,
  name,
  value,
  onChange,
}: {
  side: Side;
  name: string;
  value: number;
  onChange: (value: number) => void;
}) {
  const set = (next: number) => {
    onChange(Math.max(1, Math.min(999, next)));
    tap();
  };

  return (
    <div className={`box ${side}`}>
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
