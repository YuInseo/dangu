'use client';

import { useEffect, useMemo, useState } from 'react';

import {
  GAME_KINDS,
  formatClock,
  kindInfo,
  other,
  type GameKind,
  type GameSummary,
  type Side,
} from '../../lib/game';
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
import {
  clearHistory,
  cloudChosen,
  copyHistory,
  loadHistory,
  removeGame,
  updateGame,
} from '../../lib/storage';
import { deleteAllGames, deleteGame, pushGame } from '../../lib/firebase';
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
  /** 전체 삭제의 확인 단계. 한 번 눌러 열고, 다시 눌러야 지워진다. */
  const [wiping, setWiping] = useState(false);
  const [busy, setBusy] = useState(false);
  /** 클라우드 저장을 쓰는지 — 전체 삭제가 거기까지 미치는지를 미리 말해 주려고 본다. */
  const [cloud, setCloud] = useState(false);
  const [expanded, setExpanded] = useState<string | null>(null);
  /** 지금 고치고 있는 기록. 펼친 것과 따로 두어야 펼치기만 해서는 폼이 뜨지 않는다. */
  const [editing, setEditing] = useState<string | null>(null);

  // 보고 있는 달과, 고른 날. 달은 항상 있고 날은 없을 수 있다.
  const [cursor, setCursor] = useState(() => {
    const now = new Date();
    return { year: now.getFullYear(), month: now.getMonth() };
  });
  const [selected, setSelected] = useState<string | null>(null);
  const [showAll, setShowAll] = useState(false);

  useEffect(() => {
    void loadHistory().then(setGames);
    void cloudChosen().then(setCloud);
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

  /** 모달에 떠 있는 기록. 지워지거나 날짜를 옮기면 스스로 닫히도록 목록에서 찾는다. */
  const opened = useMemo(() => all.find((game) => game.id === expanded), [all, expanded]);

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
                  setEditing(null);
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
            <h2>
              게임 상세 <span className="count">{scope.games.length}</span>
            </h2>
            {/*
              목록만 스크롤한다.

              한 달에 수십 게임이 쌓이면 이 카드 하나가 화면 몇 개 길이가 되고, 아래에
              있는 내보내기·삭제는 그만큼 멀어진다. 카드 안에서 굴리면 카드의 크기는
              내용과 상관없이 일정하고, 화면 전체의 순서도 그대로 남는다.
            */}
            <div className="scroller">
              {scope.games.map((game) => {
                const me = game.me ?? 'white';
                const opponent = game.players[other(me)];
                const mine = game.players[me];
                const won = game.winner === me;

                return (
                  <button
                    key={game.id}
                    className="record"
                    style={{ width: '100%', background: 'none', textAlign: 'left', minHeight: 0 }}
                    onClick={() => {
                      setExpanded(game.id);
                      setEditing(null);
                      tap();
                    }}
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
                );
              })}
            </div>
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

      {all.length > 0 && (
        <div className="card">
          <h2>기록 전체 삭제</h2>
          {/*
            두 번 눌러야 지워진다.

            되돌릴 수 없는 버튼이고, 바로 위 카드에 "클립보드로 복사"가 있다 — 지우기
            전에 꺼내 둘 수 있다는 뜻이라 그 순서가 우연이 아니다. 확인 단계에서는 몇
            게임이 사라지는지와 클라우드까지 지워지는지를 말해 준다.
          */}
          {!wiping ? (
            <>
              <p>이 기기의 기록 {all.length}게임을 지웁니다. 되돌릴 수 없습니다.</p>
              <button className="danger" onClick={() => setWiping(true)}>
                전체 삭제
              </button>
            </>
          ) : (
            <>
              <p className="notice error">
                {all.length}게임을 지웁니다{cloud ? ' (클라우드 사본까지)' : ''}. 되돌릴 수
                없습니다.
              </p>
              <div className="row">
                <button
                  className="danger"
                  disabled={busy}
                  onClick={async () => {
                    setBusy(true);
                    await clearHistory();
                    if (account && cloud) await deleteAllGames(account.uid);
                    setGames([]);
                    setExpanded(null);
                    setEditing(null);
                    setWiping(false);
                    setBusy(false);
                    tap();
                  }}
                >
                  {busy ? '지우는 중…' : '정말 지웁니다'}
                </button>
                <button className="secondary" disabled={busy} onClick={() => setWiping(false)}>
                  취소
                </button>
              </div>
            </>
          )}
        </div>
      )}

      {/*
        기록 하나는 모달로 연다.

        줄 아래로 펼치던 때에는 목록이 밀려 내려가서, 열자마자 방금 누른 줄이 어디였는지
        다시 찾아야 했다. 스크롤되는 목록 안에서는 더 그렇다. 모달은 목록을 건드리지
        않고, 좁은 줄에 욱여넣을 수 없던 것들 — 핸디, 각자의 에버, 시작과 끝 시각 —
        까지 담을 자리를 준다.
      */}
      {opened && (
        <RecordSheet
          game={opened}
          editing={editing === opened.id}
          onEdit={() => {
            setEditing(opened.id);
            tap();
          }}
          onClose={() => {
            setExpanded(null);
            setEditing(null);
          }}
          onSave={async (next) => {
            const list = await updateGame(next);
            // 클라우드에도 고친 값을 올린다. 여기서 안 올리면 폰에서는 고쳐졌는데
            // 다른 기기에서는 옛날 점수가 계속 보인다.
            if (account && cloud) await pushGame(account.uid, next);
            setGames(list);
            setEditing(null);
            tap();
          }}
          onDelete={async () => {
            const next = await removeGame(opened.id);
            if (account) await deleteGame(account.uid, opened.id);
            setGames(next);
            setExpanded(null);
            setEditing(null);
          }}
        />
      )}
    </div>
  );
}

/* 기록 상세 ---------------------------------------------------------- */

/**
 * 기록 하나를 자세히 보는 모달.
 *
 * 목록의 한 줄은 상대·시각·점수 셋만 담을 수 있다. 나머지는 여기 있다: 각자의 핸디와
 * 에버, 몇 이닝을 얼마나 오래 쳤는지, 언제 시작해 언제 끝났는지, 쿠션 규칙이 있었는지.
 * 고치기와 지우기도 이 안에서 한다 — 그 대상이 눈앞에 열려 있는 자리이기 때문이다.
 */
function RecordSheet({
  game,
  editing,
  onEdit,
  onSave,
  onDelete,
  onClose,
}: {
  game: GameSummary;
  editing: boolean;
  onEdit: () => void;
  onSave: (next: GameSummary) => Promise<void>;
  onDelete: () => Promise<void>;
  onClose: () => void;
}) {
  const [confirming, setConfirming] = useState(false);

  const me = game.me ?? 'white';
  const started = new Date(game.startedAt);
  const finished = game.finishedAt ? new Date(game.finishedAt) : null;
  const innings = Math.max(1, game.inning);

  const clock = (date: Date) =>
    date.toLocaleTimeString('ko-KR', { hour: '2-digit', minute: '2-digit' });

  return (
    <div
      className="sheet"
      role="dialog"
      aria-modal="true"
      aria-label="기록 상세"
      // 바깥을 누르면 닫힌다. 안쪽에서 올라온 클릭까지 닫아 버리지 않도록 대상을 본다.
      onClick={(event) => {
        if (event.target === event.currentTarget) onClose();
      }}
    >
      <div className="inner" style={{ textAlign: 'left' }}>
        <div className="sheet-head">
          <span className="pill">{kindInfo(game.kind).label}</span>
          <strong>
            {started.toLocaleDateString('ko-KR', {
              year: 'numeric',
              month: 'long',
              day: 'numeric',
              weekday: 'short',
            })}
          </strong>
          <button className="ghost" onClick={onClose} aria-label="닫기">
            ✕
          </button>
        </div>

        {editing ? (
          <RecordEditor game={game} onCancel={onClose} onSave={onSave} />
        ) : (
          <>
            {/* 두 사람을 점수판과 같은 색으로. 어느 쪽이 누구였는지 색이 먼저 말해 준다. */}
            <div className="pair">
              {(['white', 'yellow'] as Side[]).map((side) => {
                const player = game.players[side];
                const won = game.winner === side;
                return (
                  <div className={`box ${side}`} key={side}>
                    <span className="label">
                      {side === me ? '나' : '상대'}
                      {won ? ' · 승' : game.winner ? ' · 패' : ''}
                    </span>
                    <strong className="who-name">{player.name}</strong>
                    <div className="big">{player.score}</div>
                    <span className="label">
                      핸디 {player.target} · 에버 {(player.score / innings).toFixed(3)}
                    </span>
                  </div>
                );
              })}
            </div>

            <dl className="facts">
              <div>
                <dt>결과</dt>
                <dd>{game.winner ? `${game.players[game.winner].name} 승리` : '무승부'}</dd>
              </div>
              <div>
                <dt>이닝</dt>
                <dd>{game.inning}이닝</dd>
              </div>
              <div>
                <dt>친 시간</dt>
                <dd>{formatClock(game.elapsedMs)}</dd>
              </div>
              <div>
                <dt>시작 · 끝</dt>
                <dd>
                  {clock(started)}
                  {finished ? ` · ${clock(finished)}` : ' · 끝나지 않음'}
                </dd>
              </div>
              {game.lastCushion ? (
                <div>
                  <dt>마지막 쿠션</dt>
                  <dd>{game.lastCushion}점</dd>
                </div>
              ) : null}
            </dl>

            {confirming ? (
              <>
                <p className="notice error">이 기록을 지웁니다. 되돌릴 수 없습니다.</p>
                <div className="row">
                  <button className="danger" onClick={() => void onDelete()}>
                    정말 지웁니다
                  </button>
                  <button className="secondary" onClick={() => setConfirming(false)}>
                    취소
                  </button>
                </div>
              </>
            ) : (
              <div className="row">
                <button className="secondary" onClick={onEdit}>
                  수정
                </button>
                <button className="danger" onClick={() => setConfirming(true)}>
                  삭제
                </button>
              </div>
            )}
          </>
        )}
      </div>
    </div>
  );
}

/* 기록 수정 ---------------------------------------------------------- */

/**
 * 끝난 게임 하나를 고치는 폼.
 *
 * 점수판은 실수를 한다 — 정확히는 사람이 한다. 한 점을 빼먹은 채로 끝냈거나, 상대
 * 이름을 대충 적었거나, 이닝을 몇 번 덜 세었거나. 그런 판은 지우고 다시 칠 수 없으므로
 * 고칠 수 있어야 하고, 안 그러면 틀린 숫자가 승률과 에버에 영원히 남는다.
 *
 * 승자를 점수에서 자동으로 다시 계산하지 않는 데는 이유가 있다. 점수를 고치는 이유가
 * 늘 승부를 뒤집는 건 아니고(핸디가 다르면 점수가 높은 쪽이 진 판도 있다), 무엇보다
 * 누가 이겼는지는 친 사람이 안다. 그래서 그것만은 직접 고르게 둔다.
 */
function RecordEditor({
  game,
  onSave,
  onCancel,
}: {
  game: GameSummary;
  onSave: (next: GameSummary) => Promise<void>;
  onCancel: () => void;
}) {
  const me = game.me ?? 'white';

  // 숫자도 문자열로 들고 있는다. 입력 중에 숫자로 바꿔 버리면 칸을 비우는 순간 0이
  // 들어차서, 20을 30으로 고치려고 지웠을 때 "0"부터 다시 치게 된다.
  const [draft, setDraft] = useState(() => ({
    kind: game.kind,
    white: {
      name: game.players.white.name,
      score: String(game.players.white.score),
      target: String(game.players.white.target),
    },
    yellow: {
      name: game.players.yellow.name,
      score: String(game.players.yellow.score),
      target: String(game.players.yellow.target),
    },
    inning: String(game.inning),
    minutes: String(Math.round(game.elapsedMs / 60000)),
    winner: (game.winner ?? '') as Side | '',
  }));
  const [busy, setBusy] = useState(false);

  const edit = (side: Side, field: 'name' | 'score' | 'target', value: string) =>
    setDraft((current) => ({ ...current, [side]: { ...current[side], [field]: value } }));

  /** 빈 칸이나 헛소리는 고치기 전 값으로 되돌린다 — 저장 한 번으로 기록이 망가지지 않게. */
  const number = (value: string, fallback: number, min: number) => {
    const parsed = Number(value);
    if (value.trim() === '' || !Number.isFinite(parsed)) return fallback;
    return Math.max(min, Math.round(parsed));
  };

  const box = (side: Side) => {
    const player = draft[side];
    return (
      <div className={`box ${side}`} key={side}>
        <span className="label">
          {side === 'white' ? '흰 공' : '노란 공'}
          {side === me ? ' (나)' : ''}
        </span>
        <input
          value={player.name}
          aria-label={`${side === 'white' ? '흰 공' : '노란 공'} 이름`}
          onChange={(event) => edit(side, 'name', event.target.value)}
        />
        <div className="pair">
          <label>
            <span className="label">점수</span>
            <input
              type="number"
              inputMode="numeric"
              value={player.score}
              onChange={(event) => edit(side, 'score', event.target.value)}
            />
          </label>
          <label>
            <span className="label">목표</span>
            <input
              type="number"
              inputMode="numeric"
              value={player.target}
              onChange={(event) => edit(side, 'target', event.target.value)}
            />
          </label>
        </div>
      </div>
    );
  };

  return (
    <form
      className="editor"
      onSubmit={async (event) => {
        event.preventDefault();
        if (busy) return;
        setBusy(true);
        await onSave({
          ...game,
          kind: draft.kind,
          inning: number(draft.inning, game.inning, 1),
          elapsedMs: number(draft.minutes, Math.round(game.elapsedMs / 60000), 0) * 60000,
          winner: draft.winner || undefined,
          players: {
            white: {
              name: draft.white.name.trim() || '흰 공',
              score: number(draft.white.score, game.players.white.score, 0),
              target: number(draft.white.target, game.players.white.target, 1),
            },
            yellow: {
              name: draft.yellow.name.trim() || '노란 공',
              score: number(draft.yellow.score, game.players.yellow.score, 0),
              target: number(draft.yellow.target, game.players.yellow.target, 1),
            },
          },
        });
        setBusy(false);
      }}
    >
      <span className="label">종목</span>
      <div className="choices">
        {GAME_KINDS.map((info) => (
          <button
            key={info.id}
            type="button"
            className="choice"
            aria-pressed={draft.kind === info.id}
            onClick={() => setDraft((current) => ({ ...current, kind: info.id as GameKind }))}
          >
            {info.label}
          </button>
        ))}
      </div>

      <div className="pair">{(['white', 'yellow'] as Side[]).map(box)}</div>

      <div className="pair">
        <label>
          <span className="label">이닝</span>
          <input
            type="number"
            inputMode="numeric"
            value={draft.inning}
            onChange={(event) => setDraft((current) => ({ ...current, inning: event.target.value }))}
          />
        </label>
        <label>
          <span className="label">시간(분)</span>
          <input
            type="number"
            inputMode="numeric"
            value={draft.minutes}
            onChange={(event) => setDraft((current) => ({ ...current, minutes: event.target.value }))}
          />
        </label>
      </div>

      <span className="label">승자</span>
      <div className="choices">
        {([['white', draft.white.name || '흰 공'], ['yellow', draft.yellow.name || '노란 공'], ['', '무승부']] as const).map(
          ([value, label]) => (
            <button
              key={value || 'draw'}
              type="button"
              className="choice"
              aria-pressed={draft.winner === value}
              onClick={() => setDraft((current) => ({ ...current, winner: value as Side | '' }))}
            >
              {label}
            </button>
          )
        )}
      </div>

      <div className="row">
        <button type="submit" className="primary" disabled={busy}>
          저장
        </button>
        <button type="button" className="secondary" onClick={onCancel} disabled={busy}>
          취소
        </button>
      </div>
    </form>
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
