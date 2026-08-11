'use client';

import { useEffect, useMemo, useRef, useState } from 'react';

import {
  GAME_KINDS,
  formatClock,
  highRun,
  kindInfo,
  ballOf,
  memberLines,
  other,
  sides,
  SIDE_LABELS,
  type GameKind,
  keptNotes,
  readNotes,
  type GameSummary,
  type NotePage,
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
  venueList,
  venueStats,
  type Tally,
} from '../../lib/stats';
import {
  clearHistory,
  cloudChosen,
  copyHistory,
  loadHistory,
  loadSettings,
  removeGame,
  updateGame,
} from '../../lib/storage';
import { deleteAllGames, deleteGame, pushGame } from '../../lib/firebase';
import { useAccount } from '../../lib/use-account';
import { NotePages } from '../game/notes';
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
  const { account, sync } = useAccount();
  const [games, setGames] = useState<GameSummary[] | null>(null);
  const [copied, setCopied] = useState<string | null>(null);
  /** 전체 삭제의 확인 단계. 한 번 눌러 열고, 다시 눌러야 지워진다. */
  const [wiping, setWiping] = useState(false);
  const [busy, setBusy] = useState(false);
  /** 클라우드 저장을 쓰는지 — 전체 삭제가 거기까지 미치는지를 미리 말해 주려고 본다. */
  const [cloud, setCloud] = useState(false);
  /** 설정에 적어 둔 당구장들. 아직 친 적 없는 집도 고를 수 있어야 한다. */
  const [saved, setSaved] = useState<string[]>([]);
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
  /** 당구장으로 좁혀 보기. `null`이면 전부. */
  const [place, setPlace] = useState<string | null>(null);

  useEffect(() => {
    void loadHistory().then(setGames);
    void cloudChosen().then(setCloud);
    void loadSettings().then((settings) => setSaved(settings.venues ?? []));
  }, []);

  // 맞춤이 끝나면 목록을 다시 읽는다. 맞추는 일 자체는 `useAccount`가 앱 전체에 하나로
  // 돌리므로, 이 화면은 그 결과만 받아 그린다 — 폰을 바꿨을 때 여기가 비어 있지 않은
  // 이유가 그것이다.
  useEffect(() => {
    if (!account) return;
    void loadHistory().then(setGames);
  }, [account, sync.at]);

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

  /*
    당구장으로 한 번 더 좁힌다.

    달력이 "언제"를 정하고 이것이 "어디서"를 정한다. 둘을 곱해서 쓰는 것이 자연스럽다 —
    "지난달에 대박당구장에서 몇 승 했나"는 실제로 궁금해지는 물음이고, 둘 중 하나만
    고를 수 있으면 그 물음에 답할 수 없다.
  */
  const places = useMemo(() => venueList(all, saved), [all, saved]);
  const scoped = useMemo(
    () =>
      place === null
        ? scope.games
        : scope.games.filter((game) => (game.venue ?? '').trim() === place),
    [scope.games, place]
  );

  const stats = useMemo(() => computeStats(scoped), [scoped]);
  const venues = useMemo(() => venueStats(scoped), [scoped]);
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
        <h2>
          {scope.label}
          {place ? ` · ${place}` : ''}
        </h2>

        {/* 어디서 친 것만 볼지. 장소를 적은 판이 없으면 이 줄도 없다. */}
        {places.length > 0 && (
          <div className="chips">
            <button
              className={place === null ? 'chip on' : 'chip'}
              onClick={() => {
                setPlace(null);
                tap();
              }}
            >
              전체
            </button>
            {places.map((name) => (
              <button
                key={name}
                className={place === name ? 'chip on' : 'chip'}
                onClick={() => {
                  setPlace(place === name ? null : name);
                  setExpanded(null);
                  tap();
                }}
              >
                {name}
              </button>
            ))}
          </div>
        )}

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

          {/* 어디서 쳤나. 장소를 적은 판이 하나도 없으면 이 카드는 서지 않는다 —
              적지 않는 사람에게 빈 카드를 보여 줄 이유가 없다. */}
          {venues.length > 0 && (
            <div className="card">
              <h2>당구장별</h2>
              {venues.map((place) => (
                <div className="record" key={place.name}>
                  <div className="who">
                    <strong>{place.name}</strong>
                    <span>
                      {place.games}게임 · {place.wins}승 {place.losses}패 · 에버{' '}
                      {averageOf(place).toFixed(3)}
                    </span>
                  </div>
                  <div className="result">{percent(place.rate)}</div>
                </div>
              ))}
            </div>
          )}

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
              게임 상세 <span className="count">{scoped.length}</span>
            </h2>
            {/*
              목록만 스크롤한다.

              한 달에 수십 게임이 쌓이면 이 카드 하나가 화면 몇 개 길이가 되고, 아래에
              있는 내보내기·삭제는 그만큼 멀어진다. 카드 안에서 굴리면 카드의 크기는
              내용과 상관없이 일정하고, 화면 전체의 순서도 그대로 남는다.
            */}
            <div className="scroller">
              {scoped.map((game) => {
                const me = game.me ?? 'white';
                const mine = game.players[me];
                // 셋 이상이 친 판은 상대가 하나가 아니다. 이름도 점수도 나란히 적는다.
                const rivals = sides(game)
                  .filter((side) => side !== me)
                  .map((side) => game.players[side]);
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
                      <strong>{rivals.map((player) => player?.name ?? '상대').join(' · ')}</strong>
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
                      {[mine?.score ?? 0, ...rivals.map((player) => player?.score ?? 0)].join(':')}
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
          places={places}
          onEdit={() => {
            setEditing(opened.id);
            tap();
          }}
          onClose={() => {
            setExpanded(null);
            setEditing(null);
          }}
          onNotes={async (pages) => {
            // 노트만 바꾼다. 저장은 조용히 — 여기서 진동을 울리거나 화면을 닫으면
            // 한 획 그을 때마다 그 일이 일어난다.
            const next = { ...opened, notes: keptNotes(pages) };
            const list = await updateGame(next);
            if (account && cloud) await pushGame(account.uid, next);
            setGames(list);
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
  places,
  onEdit,
  onSave,
  onNotes,
  onDelete,
  onClose,
}: {
  game: GameSummary;
  editing: boolean;
  /** 고를 수 있는 당구장들 — 적는 대신 누르는 자리를 위해. */
  places: string[];
  onEdit: () => void;
  onSave: (next: GameSummary) => Promise<void>;
  onNotes: (pages: NotePage[]) => Promise<void>;
  onDelete: () => Promise<void>;
  onClose: () => void;
}) {
  const [confirming, setConfirming] = useState(false);
  /**
   * 어느 칸을 보고 있는지.
   *
   * 한 판에 대해 알고 싶은 것이 세 종류인데 성격이 다르다 — 누가 이겼나(요약), 어떻게
   * 이겼나(이닝별), 그때 무엇을 적어 두었나(노트). 세로로 이어 붙이면 노트를 보려고
   * 매번 이닝 표를 지나쳐 내려가게 된다.
   */
  const [tab, setTab] = useState<'summary' | 'notes'>('summary');

  const me = game.me ?? 'white';
  const started = new Date(game.startedAt);
  const finished = game.finishedAt ? new Date(game.finishedAt) : null;
  const innings = Math.max(1, game.inning);
  const cushion = game.lastCushion ?? 0;

  /**
   * 이닝별 득점. 이 값이 생기기 전의 기록에는 없다.
   *
   * 합이 최종 점수와 다르면 보여 주지 않는다 — 점수를 손으로 고친 판이 그렇다. 그때의
   * 이닝별 표는 고치기 전의 이야기라, 나란히 두면 둘 중 어느 쪽이 맞는지 알 수 없는
   * 숫자 두 벌이 된다. 틀린 표를 보여 주느니 없는 편이 낫다.
   */
  const runs = useMemo(() => {
    const stored = game.runs;
    const list = sides(game);
    if (!stored || list.some((side) => !stored[side])) return null;
    const sum = (points: number[]) => points.reduce((total, value) => total + value, 0);
    const matches = list.every((side) => sum(stored[side]) === game.players[side].score);
    return matches ? stored : null;
  }, [game]);

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
      {/*
        `detail`은 넓은 화면에서 두 칸으로 눕히기 위한 표시다. 고치는 중에는 붙이지
        않는다 — 편집 폼은 한 줄짜리 입력이 세로로 쌓이는 모양이라 두 칸으로 갈라놓으면
        오히려 읽는 순서가 꼬인다.
      */}
      <div className={`inner detail${editing ? ' editing' : ''}`} style={{ textAlign: 'left' }}>
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
          <RecordEditor game={game} places={places} onCancel={onClose} onSave={onSave} />
        ) : (
          <>
            <div className="tabs" role="tablist">
              {(
                [
                  ['summary', '요약'],
                  ['notes', game.notes?.length ? `노트 ${game.notes.length}` : '노트'],
                ] as const
              ).map(([id, label]) => (
                <button
                  key={id}
                  role="tab"
                  className="tab"
                  aria-selected={tab === id}
                  onClick={() => setTab(id)}
                >
                  {label}
                </button>
              ))}
            </div>

            {/* 두 사람을 점수판과 같은 색으로. 어느 쪽이 누구였는지 색이 먼저 말해 준다. */}
            {tab === 'summary' && (
            <>
            <div className="pair" data-count={sides(game).length}>
              {sides(game).map((side) => {
                const player = game.players[side];
                const won = game.winner === side;
                // 목표를 넘긴 점수가 곧 쿠션으로 친 점수다. 규칙에 걸린 만큼까지만 센다 —
                // 이기는 순간 게임이 끝나므로 그 위로 올라갈 자리는 없다.
                const made = Math.min(cushion, Math.max(0, player.score - player.target));
                return (
                  <div className={`box ${ballOf(sides(game), side)}`} key={side}>
                    <span className="label">
                      {side === me ? '나' : '상대'}
                      {won ? ' · 승' : game.winner ? ' · 패' : ''}
                    </span>
                    {/* 팀은 자리 하나에 두 사람이다. 누가 몇 점을 쳤는지는 이 줄에만 남는다. */}
                    {player.members?.length
                      ? memberLines(player.members, runs?.[side] ?? [], innings).map((line) => (
                          <span className="label" key={line.name}>
                            {line.name} {line.points}점 · 에버 {line.average.toFixed(3)}
                          </span>
                        ))
                      : null}
                    <strong className="who-name">{player.name}</strong>
                    <div className="big">{player.score}</div>
                    <span className="label">
                      핸디 {player.target} · 에버{' '}
                      {(player.score / innings).toFixed(3).replace('-', '−')}
                    </span>
                    {cushion > 0 && (
                      <span className="label">
                        쿠션 {made}/{cushion}점
                      </span>
                    )}
                    {runs && (
                      <span className="label">하이런 {highRun(runs[side])}점</span>
                    )}
                  </div>
                );
              })}
            </div>

            <dl className="facts">
              <div>
                <dt>결과</dt>
                <dd>{game.winner ? `${game.players[game.winner].name} 승리` : '무승부'}</dd>
              </div>
              {game.venue ? (
                <div>
                  <dt>장소</dt>
                  <dd>{game.venue}</dd>
                </div>
              ) : null}
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

            {/*
              이닝별로 몇 개를 쳤는지.

              합계만 있으면 22:15가 어떻게 22:15가 되었는지는 알 수 없다 — 한 이닝에
              몰아쳤는지, 매 이닝 한 개씩 꾸준했는지가 같은 점수 안에 숨는다. 그 둘은
              당구에서 전혀 다른 판이고, 다시 볼 가치가 있는 것은 대개 그 차이다.

              공타도 줄로 남긴다. 못 친 이닝이 몇 번 이어졌는지가 그 판의 이야기의
              절반이고, 친 이닝만 추리면 1·2·7이닝처럼 번호가 튀어 그 사이가 안 보인다.
              대신 목록은 스크롤 안에 넣어 시트가 길어지지 않게 한다.
            */}
            {runs && innings > 0 && (
              <div className="runs">
                <span className="label">이닝별</span>
                <div className="grid">
                  {/*
                    줄 수는 게임의 이닝 수를 따른다. 다만 후구처럼 마지막 득점이 이닝을
                    한 칸 밀어 놓는 경우가 있어, 배열이 더 길면 그쪽을 쓴다 — 친 점수가
                    표에서 잘려 나가는 것보다 빈 줄 하나가 낫다.
                  */}
                  {Array.from(
                    {
                      length: Math.max(
                        innings,
                        ...sides(game).map((side) => runs[side]?.length ?? 0)
                      ),
                    },
                    (_, index) => (
                      <div
                        className="line"
                        key={index}
                        style={{
                          gridTemplateColumns: `3.2rem repeat(${sides(game).length}, 1fr)`,
                        }}
                      >
                        <span className="no">{index + 1}이닝</span>
                        {sides(game).map((side) => {
                          const points = runs[side]?.[index] ?? 0;
                          return (
                            <span
                              key={side}
                              className={points > 0 ? `points ${ballOf(sides(game), side)}` : 'points'}
                            >
                              {points > 0 ? points : '·'}
                            </span>
                          );
                        })}
                      </div>
                    )
                  )}
                </div>
              </div>
            )}
            </>
            )}

            {/*
              이 판의 노트.

              여기서도 고칠 수 있다. 처음에는 읽기만 하게 두었는데, 노트를 다시 여는
              이유의 절반은 뭔가를 더 적으려는 것이다 — 끝나고 나서야 생각나는 것들이
              있고, 그때 적을 자리가 없으면 노트는 치는 동안만 쓰는 것이 된다.
            */}
            {tab === 'notes' && <RecordNotes game={game} onNotes={onNotes} />}

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
/* 기록의 노트 ------------------------------------------------------- */

/**
 * 끝난 판의 노트를 다시 열고, 더 적는 자리.
 *
 * 화면이 자기 초안을 들고 있다가 잠깐 뒤에 저장한다. 노트는 한 획, 한 글자마다 바뀌는데
 * 그때마다 기록 전체를 저장소에 쓰면(클라우드까지 쓰면 더욱) 손이 느려진다. 600ms는
 * 글자를 이어 치는 동안에는 저장하지 않고, 손을 멈추면 곧 저장되는 간격이다.
 */
function RecordNotes({
  game,
  onNotes,
}: {
  game: GameSummary;
  onNotes: (pages: NotePage[]) => Promise<void>;
}) {
  const [draft, setDraft] = useState<NotePage[]>(() => readNotes(game.notes));
  const first = useRef(true);
  /*
   * 저장 함수는 ref로 들고 있는다.
   *
   * 부르는 쪽이 인라인 화살표라 그릴 때마다 새 함수가 되는데, 그걸 효과의 의존성에
   * 두면 저장 → 목록 갱신 → 다시 그림 → 또 저장이 되어 멈추지 않는다. 효과가 봐야 하는
   * 것은 "노트가 바뀌었는가" 하나뿐이다.
   */
  const save = useRef(onNotes);
  save.current = onNotes;

  useEffect(() => {
    // 처음 그린 순간에는 저장하지 않는다 — 아직 아무도 아무것도 하지 않았다.
    if (first.current) {
      first.current = false;
      return;
    }
    // 아무것도 없던 노트가 여전히 아무것도 없으면 저장할 것이 없다 — 열어 보기만 해도
    // 기록이 다시 쓰이는 일을 막는다.
    if (!keptNotes(draft) && !game.notes?.length) return;
    const timer = setTimeout(() => void save.current(draft), 600);
    return () => clearTimeout(timer);
  }, [draft]);

  return <NotePages pages={draft} onChange={setDraft} />;
}

function RecordEditor({
  game,
  places,
  onSave,
  onCancel,
}: {
  game: GameSummary;
  places: string[];
  onSave: (next: GameSummary) => Promise<void>;
  onCancel: () => void;
}) {
  const me = game.me ?? 'white';

  // 숫자도 문자열로 들고 있는다. 입력 중에 숫자로 바꿔 버리면 칸을 비우는 순간 0이
  // 들어차서, 20을 30으로 고치려고 지웠을 때 "0"부터 다시 치게 된다.
  const list = sides(game);

  const [draft, setDraft] = useState(() => ({
    kind: game.kind,
    // 자리마다 한 벌. 셋이 친 판도 넷이 친 판도 같은 모양이라, 칸 수만 달라진다.
    seats: Object.fromEntries(
      list.map((side) => [
        side,
        {
          name: game.players[side]?.name ?? '',
          score: String(game.players[side]?.score ?? 0),
          target: String(game.players[side]?.target ?? 20),
        },
      ])
    ) as Record<Side, { name: string; score: string; target: string }>,
    inning: String(game.inning),
    minutes: String(Math.round(game.elapsedMs / 60000)),
    venue: game.venue ?? '',
    winner: (game.winner ?? '') as Side | '',
  }));
  const [busy, setBusy] = useState(false);

  const edit = (side: Side, field: 'name' | 'score' | 'target', value: string) =>
    setDraft((current) => ({
      ...current,
      seats: { ...current.seats, [side]: { ...current.seats[side], [field]: value } },
    }));

  /** 빈 칸이나 헛소리는 고치기 전 값으로 되돌린다 — 저장 한 번으로 기록이 망가지지 않게. */
  const number = (value: string, fallback: number, min: number) => {
    const parsed = Number(value);
    if (value.trim() === '' || !Number.isFinite(parsed)) return fallback;
    return Math.max(min, Math.round(parsed));
  };

  const box = (side: Side) => {
    const player = draft.seats[side];
    const label = SIDE_LABELS[side] ?? side;
    return (
      <div className={`box ${ballOf(list, side)}`} key={side}>
        <span className="label">
          {label}
          {side === me ? ' (나)' : ''}
        </span>
        <input
          value={player.name}
          aria-label={`${label} 이름`}
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
          venue: draft.venue.trim() || undefined,
          winner: draft.winner || undefined,
          players: Object.fromEntries(
            list.map((side) => [
              side,
              {
                ...game.players[side],
                name: draft.seats[side].name.trim() || SIDE_LABELS[side] || side,
                score: number(draft.seats[side].score, game.players[side]?.score ?? 0, 0),
                target: number(draft.seats[side].target, game.players[side]?.target ?? 20, 1),
              },
            ])
          ),
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

      <div className="pair" data-count={list.length}>{list.map(box)}</div>

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

      <label>
        <span className="label">당구장</span>
        <input
          value={draft.venue}
          placeholder="적지 않아도 됩니다"
          onChange={(event) => setDraft((current) => ({ ...current, venue: event.target.value }))}
        />
      </label>

      {/* 다녀온 집들. 고친다는 것은 대개 "빼먹은 것을 채운다"는 뜻이고, 그때 적을 이름은
          거의 언제나 이미 아는 집이다 — 누르는 편이 적는 것보다 빠르고 정확하다. */}
      {places.length > 0 && (
        <div className="chips">
          {places.map((name) => (
            <button
              key={name}
              type="button"
              className={draft.venue.trim() === name ? 'chip on' : 'chip'}
              onClick={() =>
                setDraft((current) => ({
                  ...current,
                  venue: current.venue.trim() === name ? '' : name,
                }))
              }
            >
              {name}
            </button>
          ))}
        </div>
      )}

      <span className="label">승자</span>
      <div className="choices">
        {([
          ...list.map(
            (side): [Side | '', string] => [side, draft.seats[side].name || SIDE_LABELS[side] || side]
          ),
          ['', '무승부'] as [Side | '', string],
        ]).map(
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
