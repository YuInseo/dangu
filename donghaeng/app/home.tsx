'use client';

import { useEffect, useState } from 'react';

import { cleanCode, createTeam, joinTeam, reason } from '../lib/firebase';
import { prepareShell, tap } from '../lib/platform';
import { useSession, type Session } from '../lib/session';
import { Board } from './board';

/**
 * 첫 화면이 무엇일지 정한다.
 *
 * 이름과 팀이 있으면 곧장 쪽지판이다. 매번 켤 때마다 지나가는 화면이 하나라도 있으면,
 * 열차가 들어오는 중에 그 화면을 넘기고 있게 된다.
 */
export function Home() {
  const session = useSession();

  useEffect(() => {
    void prepareShell();
  }, []);

  if (session.loading) {
    return (
      <div className="app center">
        <p className="muted">불러오는 중…</p>
      </div>
    );
  }

  if (!session.account) return <SignIn session={session} />;
  if (!session.name || !session.team) return <Setup session={session} />;
  return <Board session={session} />;
}

/**
 * 로그인. 구글 계정 하나로 끝난다.
 *
 * 한 번 로그인하면 기기에 남으므로 이 화면은 처음에만 본다.
 */
function SignIn({ session }: { session: Session }) {
  return (
    <div className="app">
      <header className="topbar">
        <h1>동행</h1>
      </header>

      <main className="setup">
        <p className="lead">
          승하차 도움 쪽지를 팀이 같이 봅니다. 한 사람이 적으면 모두의 폰에 바로 뜹니다.
        </p>

        <button
          className="google wide"
          disabled={session.signingIn}
          onClick={() => {
            tap();
            void session.signIn();
          }}
        >
          <span className="g" aria-hidden="true">
            G
          </span>
          {session.signingIn ? '로그인 중…' : 'Google로 로그인'}
        </button>

        {session.error && <p className="notice error">{session.error}</p>}
      </main>
    </div>
  );
}

/**
 * 처음 한 번: 이름, 그리고 팀.
 *
 * 팀은 대개 누군가 이미 만들어 두었다. 그래서 "코드로 들어가기"가 위에 있다. 만드는 쪽은
 * 역에 한 사람이면 된다.
 */
function Setup({ session }: { session: Session }) {
  const [name, setName] = useState(session.name ?? '');
  const [code, setCode] = useState('');
  const [teamName, setTeamName] = useState('');
  const [mode, setMode] = useState<'join' | 'create'>('join');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const ready = name.trim().length > 0 && Boolean(session.uid) && !busy;

  const submit = async () => {
    if (!session.uid) return;
    setBusy(true);
    setError(null);
    tap();
    const me = { uid: session.uid, name: name.trim() };
    try {
      await session.setName(me.name);
      if (mode === 'join') {
        const team = await joinTeam(code, me);
        if (!team) {
          setError('그런 팀 코드가 없습니다. 여덟 글자를 다시 확인하세요.');
          return;
        }
        await session.setTeam(team);
      } else {
        const team = await createTeam(teamName, me);
        await session.setTeam(team);
      }
    } catch (failure) {
      setError(reason(failure));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="app">
      <header className="topbar">
        <h1>동행</h1>
      </header>

      <main className="setup">
        <p className="lead">
          승하차 도움 쪽지를 팀이 같이 봅니다. 한 사람이 적으면 모두의 폰에 바로 뜹니다.
        </p>

        <label className="field">
          <span>내 이름</span>
          <input
            value={name}
            placeholder="예: 김역무"
            autoComplete="name"
            onChange={(event) => setName(event.target.value)}
          />
          <small>쪽지를 맡거나 적을 때 이 이름이 붙습니다.</small>
        </label>

        <div className="segmented" role="tablist">
          <button
            role="tab"
            aria-selected={mode === 'join'}
            className={mode === 'join' ? 'on' : ''}
            onClick={() => setMode('join')}
          >
            코드로 들어가기
          </button>
          <button
            role="tab"
            aria-selected={mode === 'create'}
            className={mode === 'create' ? 'on' : ''}
            onClick={() => setMode('create')}
          >
            새 팀 만들기
          </button>
        </div>

        {mode === 'join' ? (
          <label className="field">
            <span>팀 코드</span>
            <input
              className="code"
              value={code}
              placeholder="ABCD-2345"
              autoCapitalize="characters"
              autoCorrect="off"
              spellCheck={false}
              maxLength={12}
              onChange={(event) => setCode(event.target.value)}
            />
            <small>팀을 만든 사람의 설정 화면에 있습니다.</small>
          </label>
        ) : (
          <label className="field">
            <span>팀 이름</span>
            <input
              value={teamName}
              placeholder="예: 수원역 고객지원"
              onChange={(event) => setTeamName(event.target.value)}
            />
            <small>만들면 여덟 글자 코드가 나옵니다. 그 코드를 동료에게 알려 주세요.</small>
          </label>
        )}

        <button
          className="primary wide"
          disabled={
            !ready || (mode === 'join' ? cleanCode(code).length !== 8 : teamName.trim().length === 0)
          }
          onClick={() => void submit()}
        >
          {busy ? '잠시만요…' : mode === 'join' ? '들어가기' : '만들기'}
        </button>

        {error && <p className="notice error">{error}</p>}

        <p className="muted account-line">
          {session.account?.email ?? session.account?.name} 계정으로 로그인됨 ·{' '}
          <button className="link" onClick={() => void session.signOut()}>
            다른 계정
          </button>
        </p>
      </main>
    </div>
  );
}
