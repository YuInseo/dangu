'use client';

import { useEffect, useMemo, useState } from 'react';

import { formatClock, kindInfo, other, type GameSummary } from '../../lib/game';
import {
  WEEKDAYS,
  averageOf,
  computeStats,
  dayKey,
  humanDuration,
  monthGrid,
  monthKey,
  percent,
  tallyByDay,
  type Tally,
} from '../../lib/stats';
import { cloudChosen, copyHistory, loadHistory, removeGame } from '../../lib/storage';
import { deleteGame } from '../../lib/firebase';
import { syncDown, useAccount } from '../../lib/use-account';
import { tap } from '../../lib/platform';

/**
 * 통계.
 *
 * 맨 위가 달력이고, 나머지는 전부 그 달력이 가리키는 범위의 이야기다. 날짜를 고르면
 * 그 날, 안 고르면 보고 있는 달, 그리고 "전체"를 누르면 전부 — 아래 카드들이 무엇을
 * 세고 있는지 화면 맨 위만 보면 알 수 있게 했다.
 *
 * 계산은 전부 `lib/stats.ts`에서 하고 여기서는 그리기만 한다. 승률 같은 숫자는 한 번
 * 틀리면 조용히 틀린 채로 남기 때문에, 화면 코드 사이에 계산이 섞여 있으면 안 된다.
 */
export function StatsView() {
  const { account } = useAccount();
  const [games, setGames] = useState<GameSummary[] | null>(null);
  const [copied, setCopied] = useState<string | null>(null);
  const [expanded, setExpanded] = useState<string | null>(null);

  // 보고 있는 달과, 고른 날. 달은 항상 있고 날은 없을 수 있다.
  const [cursor, setCursor] = useState(() => {
    const now = new Date();
    return { year: now.getFullYear(), month: now.getMonth() };
  });
  const [selected, setSelected] = useState<string | null>(null);
  const [showAll, setShowAll] = useState(false);

  useEffect(() => {
    void loadHistory().then(setGames);
  }, []);

  // 클라우드 저장을 고른 사람에게는 계정에 있는 것까지 합쳐 본다. 폰을 바꿨을 때
  // 이 화면이 비어 있지 않은 이유가 이 효과다.
  useEffect(() => {
    if (!account) return;
    void cloudChosen().then((yes) => {
      if (yes) void syncDown(account.uid).then(setGames);
    });
  }, [account]);

  const all = games ?? [];
  const byDay = useMemo(() => tallyByDay(all), [all]);
  const cells = useMemo(
    () => monthGrid(cursor.year, cursor.month, byDay),
    [cursor.year, cursor.month, byDay]
  );

  const viewMonth = `${cursor.year}-${String(cursor.month + 1).padStart(2, '0')}`;

  /** 아래 카드들이 세는 범위. 달력이 정하고, "전체"가 덮는다. */
  const scope = useMemo(() => {
    if (showAll) return { label: '전체', games: all };
    if (selected) {
      return { label: labelForDay(selected), games: all.filter((game) => dayKey(game.startedAt) === selected) };
    }
    return {
      label: `${cursor.year}년 ${cursor.month + 1}월`,
      games: all.filter((game) => monthKey(game.startedAt) === viewMonth),
    };
  }, [showAll, selected, all, cursor, viewMonth]);

  const stats = useMemo(() => computeStats(scope.games), [scope.games]);
  const overall = useMemo(() => computeStats(all), [all]);

  const shiftMonth = (delta: number) => {
    setSelected(null);
    setShowAll(false);
    setCursor((current) => {
      const date = new Date(current.year, current.month + delta, 1);
      return { year: date.getFullYear(), month: date.getMonth() };
    });
    tap();
  };

  if (!games) return <div className="page" />;

  const today = dayKey(Date.now());

  return (
    <div className="page">
      <div className="card">
        <div className="calendar-head">
          <button className="calendar-nav" onClick={() => shiftMonth(-1)} aria-label="이전 달">
            ‹
          </button>
          <strong>
            {cursor.year}년 {cursor.month + 1}월
          </strong>
          <button className="calendar-nav" onClick={() => shiftMonth(1)} aria-label="다음 달">
            ›
          </button>
        </div>

        <div className="calendar">
          {WEEKDAYS.map((label) => (
            <div className="weekday" key={label}>
              {label}
            </div>
          ))}

          {cells.map((cell, index) =>
            cell.key === null ? (
              <div key={`empty-${index}`} className="day empty" />
            ) : (
              <button
                key={cell.key}
                className={[
                  'day',
                  cell.games > 0 ? 'played' : '',
                  cell.key === selected && !showAll ? 'selected' : '',
                  cell.key === today ? 'today' : '',
                ]
                  .filter(Boolean)
                  .join(' ')}
                // 같은 날을 다시 누르면 선택이 풀린다 — 되돌아가려고 다른 버튼을
                // 찾게 만들지 않는다.
                onClick={() => {
                  setShowAll(false);
                  setSelected((current) => (current === cell.key ? null : cell.key));
                  setExpanded(null);
                  tap();
                }}
              >
                <span className="n">{cell.day}</span>
                {cell.games > 0 && (
                  <span className="dots" aria-label={`${cell.games}게임`}>
                    {cell.games > 3 ? `${cell.games}` : '•'.repeat(cell.games)}
                  </span>
                )}
              </button>
            )
          )}
        </div>

        <div className="row">
          <button
            className="secondary"
            onClick={() => {
              const now = new Date();
              setCursor({ year: now.getFullYear(), month: now.getMonth() });
              setSelected(today);
              setShowAll(false);
              tap();
            }}
          >
            오늘
          </button>
          <button
            className="secondary"
            aria-pressed={showAll}
            onClick={() => {
              setShowAll((current) => !current);
              setSelected(null);
              tap();
            }}
          >
            전체 기간
          </button>
          {selected && !showAll && (
            <button className="ghost" onClick={() => setSelected(null)}>
              날짜 해제
            </button>
          )}
        </div>
      </div>

      <div className="card">
        <h2>{scope.label}</h2>
        <TallyRow tally={stats.all} />
        {stats.all.games === 0 && <p>이 기간에는 친 기록이 없습니다.</p>}
        {stats.all.games > 0 && (
          <p>
            에버 {averageOf(stats.all).toFixed(3)} · {stats.all.innings}이닝
            {stats.currentStreak > 1 ? ` · ${stats.currentStreak}연승 중` : ''}
          </p>
        )}
        {!showAll && (
          <p style={{ fontSize: '0.8rem' }}>
            누적 {overall.all.games}게임 · {humanDuration(overall.all.elapsedMs)} · 전체 승률{' '}
            {percent(overall.all.rate)}
          </p>
        )}
        {stats.recentForm.length > 0 && (
          <div className="row" style={{ gap: '0.3rem' }}>
            {stats.recentForm.map((result, index) => (
              <span
                key={index}
                className="pill"
                style={{
                  flex: '0 0 auto',
                  background:
                    result === 'W' ? 'rgba(22,163,74,0.25)' : result === 'L' ? 'rgba(220,38,38,0.2)' : undefined,
                }}
              >
                {result === 'W' ? '승' : result === 'L' ? '패' : '무'}
              </span>
            ))}
          </div>
        )}
      </div>

      {stats.all.games > 0 && (
        <>
          <div className="card">
            <h2>상대별</h2>
            {stats.opponents.map((opponent) => (
              <div className="record" key={opponent.name}>
                <div className="who">
                  <strong>{opponent.name}</strong>
                  <span>
                    {opponent.games}게임 · {opponent.wins}승 {opponent.losses}패
                  </span>
                </div>
                <div className="result">{percent(opponent.rate)}</div>
              </div>
            ))}
          </div>

          <div className="card">
            <h2>종목별</h2>
            {stats.kinds.map((kind) => (
              <div className="record" key={kind.kind}>
                <div className="who">
                  <strong>{kind.label}</strong>
                  <span>
                    {kind.games}게임 · 에버 {averageOf(kind).toFixed(3)}
                  </span>
                </div>
                <div className="result">{kind.games === 0 ? '—' : percent(kind.rate)}</div>
              </div>
            ))}
          </div>

          <div className="card">
            <h2>게임 상세</h2>
            {scope.games.map((game) => {
              const me = game.me ?? 'white';
              const opponent = game.players[other(me)];
              const mine = game.players[me];
              const won = game.winner === me;
              const open = expanded === game.id;

              return (
                <div key={game.id}>
                  <button
                    className="record"
                    style={{ width: '100%', background: 'none', textAlign: 'left', minHeight: 0 }}
                    onClick={() => setExpanded(open ? null : game.id)}
                  >
                    <span className="pill">{kindInfo(game.kind).label}</span>
                    <span className="who">
                      <strong>{opponent.name}</strong>
                      <span>
                        {new Date(game.startedAt).toLocaleString('ko-KR', {
                          month: 'numeric',
                          day: 'numeric',
                          hour: '2-digit',
                          minute: '2-digit',
                        })}
                      </span>
                    </span>
                    <span className="result" style={{ color: won ? '#4ade80' : '#f87171' }}>
                      {mine.score}:{opponent.score}
                    </span>
                  </button>

                  {open && (
                    <div className="notice" style={{ marginBottom: '0.6rem' }}>
                      {mine.name} {mine.score}/{mine.target} · {opponent.name} {opponent.score}/
                      {opponent.target}
                      <br />
                      {game.inning}이닝 · {formatClock(game.elapsedMs)} · 에버{' '}
                      {(mine.score / Math.max(1, game.inning)).toFixed(3)}
                      {game.lastCushion ? ` · 마지막 쿠션 ${game.lastCushion}점` : ''}
                      <br />
                      {game.winner ? `${game.players[game.winner].name} 승리` : '무승부'}
                      <div className="row" style={{ marginTop: '0.5rem' }}>
                        <button
                          className="danger"
                          onClick={async () => {
                            const next = await removeGame(game.id);
                            if (account) await deleteGame(account.uid, game.id);
                            setGames(next);
                            setExpanded(null);
                          }}
                        >
                          이 기록 삭제
                        </button>
                      </div>
                    </div>
                  )}
                </div>
              );
            })}
          </div>
        </>
      )}

      <div className="card">
        <h2>내보내기</h2>
        <p>전체 기록을 클립보드로 복사합니다. 엑셀이나 메모장에 그대로 붙습니다.</p>
        <button
          className="secondary"
          onClick={async () => {
            const result = await copyHistory();
            setCopied(result.supported ? '복사했습니다.' : (result.reason ?? '복사하지 못했습니다.'));
          }}
        >
          클립보드로 복사
        </button>
        {copied && <p className="notice">{copied}</p>}
      </div>
    </div>
  );
}

/** `2026-08-09` → `8월 9일 (토)`. */
function labelForDay(key: string): string {
  const [year, month, day] = key.split('-').map(Number);
  const date = new Date(year, month - 1, day);
  return `${month}월 ${day}일 (${WEEKDAYS[date.getDay()]})`;
}

function TallyRow({ tally }: { tally: Tally }) {
  return (
    <div className="row">
      <Figure label="게임" value={String(tally.games)} />
      <Figure label="승" value={String(tally.wins)} />
      <Figure label="패" value={String(tally.losses)} />
      <Figure label="승률" value={percent(tally.rate)} />
      <Figure label="시간" value={humanDuration(tally.elapsedMs)} />
    </div>
  );
}

function Figure({ label, value }: { label: string; value: string }) {
  return (
    <div style={{ textAlign: 'center', minWidth: '3.5rem' }}>
      <div style={{ fontSize: '1.35rem', fontWeight: 800, fontVariantNumeric: 'tabular-nums' }}>{value}</div>
      <div style={{ fontSize: '0.72rem', color: 'rgba(243,244,246,0.55)' }}>{label}</div>
    </div>
  );
}
