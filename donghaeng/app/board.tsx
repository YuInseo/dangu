'use client';

import Link from 'graft/link';
import { useEffect, useMemo, useRef, useState } from 'react';

import { sweepOldSlips, watchSlips } from '../lib/firebase';
import { onBack, tap } from '../lib/platform';
import type { Session } from '../lib/session';
import {
  KIND_LABEL,
  dateKey,
  dateTitle,
  shiftDate,
  sortSlips,
  type Kind,
  type Slip,
} from '../lib/slip';
import { SlipCard } from './slip-card';
import { SlipEditor } from './slip-editor';
import { SlipSheet } from './slip-sheet';

type Filter = 'all' | Kind;

/**
 * 쪽지판 — 하루치 쪽지가 열차 시각 순서로.
 *
 * 책상 위에 쪽지를 늘어놓던 것과 같다. 다른 점은 그 책상이 팀 모두의 주머니에 있다는 것:
 * 누가 쪽지를 적거나 맡거나 끝내면 다른 사람의 화면에서도 그 자리에서 바뀐다.
 */
export function Board({ session }: { session: Session }) {
  const team = session.team!;
  const me = session.me;
  const [date, setDate] = useState(dateKey);
  const [slips, setSlips] = useState<Slip[]>([]);
  const [synced, setSynced] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [filter, setFilter] = useState<Filter>('all');
  const [hideDone, setHideDone] = useState(false);
  const [openId, setOpenId] = useState<string | null>(null);
  const [editing, setEditing] = useState<{ kind: Kind; slip?: Slip } | null>(null);
  const [toast, setToast] = useState<string | null>(null);
  const [fresh, setFresh] = useState<Set<string>>(new Set());
  const [, setTick] = useState(0);
  const primed = useRef(false);

  // "12분 후"가 멈춰 있으면 안 된다. 분 단위 글자라 30초면 충분하다.
  useEffect(() => {
    const timer = setInterval(() => setTick((value) => value + 1), 30_000);
    return () => clearInterval(timer);
  }, []);

  // 자정을 넘겨 켜 둔 화면이 어제에 머물지 않게. 보고 있던 날이 "오늘"이었을 때만 넘긴다.
  useEffect(() => {
    const timer = setInterval(() => {
      const today = dateKey();
      setDate((current) => (current === shiftDate(today, -1) ? today : current));
    }, 60_000);
    return () => clearInterval(timer);
  }, []);

  useEffect(() => {
    void sweepOldSlips(team.code);
  }, [team.code]);

  useEffect(() => {
    let stop = () => {};
    let alive = true;
    primed.current = false;
    setSynced(false);
    setSlips([]);

    void watchSlips(
      team.code,
      date,
      (next, added, fromServer) => {
        if (!alive) return;
        setSlips(next);
        setError(null);
        if (fromServer) setSynced(true);

        // 처음 받은 목록은 "새 쪽지"가 아니다. 서버에서 한 번 확인된 뒤부터 센다.
        if (!primed.current) {
          if (fromServer) primed.current = true;
          return;
        }
        const theirs = added.filter((slip) => slip.createdBy.uid !== me?.uid);
        if (theirs.length === 0) return;
        tap('heavy');
        const first = theirs[0];
        setToast(
          `새 쪽지 · ${KIND_LABEL[first.kind]} ${first.time}${first.train ? ` ${first.train}열차` : ''}` +
            (theirs.length > 1 ? ` 외 ${theirs.length - 1}장` : '')
        );
        setFresh((current) => new Set([...current, ...theirs.map((slip) => slip.id)]));
      },
      (message) => alive && setError(message)
    ).then((unsubscribe) => {
      if (alive) stop = unsubscribe;
      else unsubscribe();
    });

    return () => {
      alive = false;
      stop();
    };
  }, [team.code, date, me?.uid]);

  useEffect(() => {
    if (!toast) return;
    const timer = setTimeout(() => setToast(null), 4000);
    return () => clearTimeout(timer);
  }, [toast]);

  const counts = useMemo(
    () => ({
      all: slips.length,
      depart: slips.filter((slip) => slip.kind === 'depart').length,
      arrive: slips.filter((slip) => slip.kind === 'arrive').length,
      open: slips.filter((slip) => slip.status !== 'done').length,
    }),
    [slips]
  );

  const visible = useMemo(
    () =>
      sortSlips(slips).filter(
        (slip) => (filter === 'all' || slip.kind === filter) && !(hideDone && slip.status === 'done')
      ),
    [slips, filter, hideDone]
  );

  const opened = openId ? (slips.find((slip) => slip.id === openId) ?? null) : null;
  const today = dateKey();

  // 시트가 열려 있으면 뒤로 가기는 시트를 닫는다. 앱이 꺼지면 적던 쪽지가 날아간다.
  const sheetOpen = Boolean(opened || editing);
  useEffect(() => {
    if (!sheetOpen) return;
    return onBack(() => {
      setEditing(null);
      setOpenId(null);
    });
  }, [sheetOpen]);

  const open = (slip: Slip) => {
    tap();
    setOpenId(slip.id);
    setFresh((current) => {
      if (!current.has(slip.id)) return current;
      const next = new Set(current);
      next.delete(slip.id);
      return next;
    });
  };

  return (
    <div className="app">
      <header className="topbar">
        <div className="title">
          <h1>{team.name || '동행'}</h1>
          <span className={synced ? 'sync on' : 'sync'}>{synced ? '실시간' : '연결 중…'}</span>
        </div>
        <Link className="ghost" href="/settings" aria-label="설정">
          설정
        </Link>
      </header>

      <nav className="datebar" aria-label="날짜">
        <button aria-label="전날" onClick={() => setDate((current) => shiftDate(current, -1))}>
          ‹
        </button>
        <button className="date" onClick={() => setDate(today)} disabled={date === today}>
          <strong>{dateTitle(date, today)}</strong>
          {date !== today && <small>오늘로</small>}
        </button>
        <button aria-label="다음 날" onClick={() => setDate((current) => shiftDate(current, 1))}>
          ›
        </button>
      </nav>

      <div className="filters">
        <div className="segmented small">
          {(['all', 'depart', 'arrive'] as const).map((value) => (
            <button key={value} className={filter === value ? 'on' : ''} onClick={() => setFilter(value)}>
              {value === 'all' ? '전체' : KIND_LABEL[value]} <b>{counts[value]}</b>
            </button>
          ))}
        </div>
        <label className="toggle">
          <input type="checkbox" checked={hideDone} onChange={(event) => setHideDone(event.target.checked)} />
          완료 숨김
        </label>
      </div>

      <main className="board">
        {session.error && (
          <div className="notice error">
            <p>{session.error}</p>
            <button className="secondary" onClick={session.retry}>
              다시 시도
            </button>
          </div>
        )}
        {error && !session.error && <p className="notice error">{error}</p>}

        {visible.length === 0 ? (
          <div className="empty">
            <p>{slips.length === 0 ? `${dateTitle(date, today)} 쪽지가 없습니다.` : '조건에 맞는 쪽지가 없습니다.'}</p>
            {slips.length === 0 && <p className="muted">아래 버튼으로 첫 쪽지를 적으세요.</p>}
          </div>
        ) : (
          <ul className="slips">
            {visible.map((slip) => (
              <li key={slip.id}>
                <SlipCard slip={slip} mine={slip.assignee?.uid === me?.uid} fresh={fresh.has(slip.id)} onOpen={open} />
              </li>
            ))}
          </ul>
        )}
        {counts.open > 0 && <p className="tally">남은 쪽지 {counts.open}장</p>}
      </main>

      <footer className="dock">
        <button className="add depart" disabled={!me} onClick={() => (tap(), setEditing({ kind: 'depart' }))}>
          + 출발 쪽지
        </button>
        <button className="add arrive" disabled={!me} onClick={() => (tap(), setEditing({ kind: 'arrive' }))}>
          + 도착 쪽지
        </button>
      </footer>

      {opened && me && (
        <SlipSheet
          slip={opened}
          team={team}
          me={me}
          onClose={() => setOpenId(null)}
          onEdit={() => {
            setEditing({ kind: opened.kind, slip: opened });
            setOpenId(null);
          }}
          onNotice={setToast}
        />
      )}

      {editing && me && (
        <SlipEditor
          kind={editing.kind}
          slip={editing.slip}
          date={date}
          team={team}
          me={me}
          stations={[...new Set(slips.map((slip) => slip.station).filter(Boolean))]}
          onClose={() => setEditing(null)}
          onSaved={(message) => {
            setEditing(null);
            setToast(message);
          }}
        />
      )}

      {toast && (
        <div className="toast" role="status" onClick={() => setToast(null)}>
          {toast}
        </div>
      )}
    </div>
  );
}
