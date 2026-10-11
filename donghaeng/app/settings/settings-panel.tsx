'use client';

import Link from 'graft/link';
import { useRouter } from 'graft/navigation';
import { useEffect, useState } from 'react';

import { leaveTeam, renameMember, watchMembers } from '../../lib/firebase';
import { shareText, tap } from '../../lib/platform';
import { useSession } from '../../lib/session';
import { RETENTION_DAYS, type Person } from '../../lib/slip';

/** `ABCD2345` → `ABCD-2345`. 불러 줄 때 네 글자씩 끊는 편이 덜 틀린다. */
const pretty = (code: string) => `${code.slice(0, 4)}-${code.slice(4)}`;

/**
 * 설정: 내 이름, 팀 코드, 누가 같이 보고 있는지.
 *
 * 팀 코드를 동료에게 알려 주는 것이 이 화면의 주된 쓰임이다. 그래서 코드가 맨 위에 크게
 * 있고, 공유 버튼이 그 바로 옆에 있다.
 */
export function SettingsPanel() {
  const session = useSession();
  const router = useRouter();
  const [name, setName] = useState(session.name ?? '');
  const [members, setMembers] = useState<Person[]>([]);
  const [notice, setNotice] = useState<string | null>(null);
  const [leaving, setLeaving] = useState(false);

  useEffect(() => setName(session.name ?? ''), [session.name]);

  useEffect(() => {
    if (!session.team || !session.uid) return;
    let stop = () => {};
    let alive = true;
    void watchMembers(session.team.code, (list) => alive && setMembers(list)).then((unsubscribe) => {
      if (alive) stop = unsubscribe;
      else unsubscribe();
    });
    return () => {
      alive = false;
      stop();
    };
  }, [session.team?.code, session.uid]);

  const team = session.team;

  const saveName = async () => {
    const clean = name.trim();
    if (!clean || clean === session.name) return;
    tap();
    await session.setName(clean);
    if (team && session.uid) void renameMember(team.code, { uid: session.uid, name: clean });
    setNotice('이름을 바꿨습니다. 앞으로 적는 쪽지에 붙습니다.');
  };

  const invite = async () => {
    if (!team) return;
    tap();
    const result = await shareText(
      '동행 팀 코드',
      `동행 앱에서 "${team.name}" 팀에 들어오세요.\n팀 코드: ${pretty(team.code)}`
    );
    if (result === 'copied') setNotice('팀 코드를 복사했습니다.');
    if (result === 'failed') setNotice(`팀 코드: ${pretty(team.code)}`);
  };

  const leave = async () => {
    if (!team) return;
    if (!leaving) {
      setLeaving(true);
      return;
    }
    tap('heavy');
    if (session.uid) await leaveTeam(team.code, session.uid);
    await session.setTeam(null);
    router.push('/');
  };

  return (
    <div className="app">
      <header className="topbar">
        <Link className="ghost" href="/" aria-label="뒤로">
          ←
        </Link>
        <h1>설정</h1>
      </header>

      <main className="settings">
        {team && (
          <section className="panel">
            <h2>팀</h2>
            <p className="team-name">{team.name}</p>
            <div className="code-row">
              <span className="team-code" aria-label="팀 코드">
                {pretty(team.code)}
              </span>
              <button className="primary" onClick={() => void invite()}>
                코드 보내기
              </button>
            </div>
            <p className="muted">이 코드를 아는 사람은 누구나 팀의 쪽지를 볼 수 있습니다. 팀 밖으로 돌리지 마세요.</p>

            {members.length > 0 && (
              <>
                <h3>같이 보는 사람 {members.length}명</h3>
                <ul className="members">
                  {members.map((member) => (
                    <li key={member.uid}>
                      {member.name || '이름 없음'}
                      {member.uid === session.uid && <span className="muted"> (나)</span>}
                    </li>
                  ))}
                </ul>
              </>
            )}
          </section>
        )}

        <section className="panel">
          <h2>내 이름</h2>
          <div className="code-row">
            <input value={name} onChange={(event) => setName(event.target.value)} aria-label="내 이름" />
            <button className="secondary" disabled={!name.trim() || name.trim() === session.name} onClick={() => void saveName()}>
              저장
            </button>
          </div>
        </section>

        {notice && <p className="notice">{notice}</p>}

        <section className="panel">
          <h2>개인정보</h2>
          <p className="muted">
            쪽지에는 고객 연락처가 들어갑니다. 쪽지는 {RETENTION_DAYS}일이 지나면 팀의 누군가가 앱을 열 때 자동으로
            지워집니다 — 오늘과 어제 것만 남습니다.
          </p>
        </section>

        {team && (
          <section className="panel">
            <h2>팀 나가기</h2>
            <p className="muted">나가도 쪽지는 팀에 남습니다. 코드를 다시 넣으면 돌아올 수 있습니다.</p>
            <button className={leaving ? 'danger wide' : 'secondary wide'} onClick={() => void leave()}>
              {leaving ? '정말 나가기' : '이 팀에서 나가기'}
            </button>
          </section>
        )}

        <p className="version muted">동행 v{process.env.GRAFT_PUBLIC_APP_VERSION}</p>
      </main>
    </div>
  );
}
