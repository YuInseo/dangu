'use client';

import { useEffect, useMemo, useState } from 'react';

import { onBack, tap } from '../../lib/platform';
import {
  SHIFTS,
  SHIFT_LABEL,
  SHIFT_NAME,
  anchorFor,
  loadSchedule,
  parsePattern,
  patternText,
  saveSchedule,
  shiftOn,
  type Schedule,
  type Shift,
} from '../../lib/shift';
import { dateKey } from '../../lib/slip';
import { TabBar } from '../tab-bar';

// 자주 쓰는 순서. 이 밖의 순서는 글자로 직접 적는다.
const PRESETS = ['주야비휴', '주주야야비휴휴', '주야비휴휴'];

const pad = (value: number) => String(value).padStart(2, '0');

/** 그 달의 칸들. 앞뒤는 빈칸(`null`)으로 채워 일요일부터 시작하는 주로 맞춘다. */
function monthCells(year: number, month: number): (string | null)[] {
  const first = new Date(year, month - 1, 1).getDay();
  const days = new Date(year, month, 0).getDate();
  const cells: (string | null)[] = Array(first).fill(null);
  for (let day = 1; day <= days; day++) cells.push(`${year}-${pad(month)}-${pad(day)}`);
  while (cells.length % 7) cells.push(null);
  return cells;
}

/**
 * 근무표 — 한 달 달력에 그날의 주·야·비·휴.
 *
 * 처음에는 순서와 "오늘 무슨 근무인지"만 묻는다. 그다음부터는 달력이 알아서 채운다.
 * 대근이나 연차로 바뀐 날은 그 날을 눌러 고친다.
 */
export function CalendarView() {
  const today = dateKey();
  const [schedule, setSchedule] = useState<Schedule | null>(null);
  const [month, setMonth] = useState(() => {
    const now = new Date();
    return { year: now.getFullYear(), month: now.getMonth() + 1 };
  });
  const [picking, setPicking] = useState<string | null>(null);
  const [setting, setSetting] = useState(false);

  useEffect(() => {
    void loadSchedule().then((loaded) => {
      setSchedule(loaded);
      if (!loaded.anchor) setSetting(true);
    });
  }, []);

  useEffect(() => {
    if (!picking) return;
    return onBack(() => setPicking(null));
  }, [picking]);

  const cells = useMemo(() => monthCells(month.year, month.month), [month]);

  const update = (next: Schedule) => {
    setSchedule(next);
    void saveSchedule(next);
  };

  const moveMonth = (delta: number) => {
    tap();
    setMonth(({ year, month: current }) => {
      const date = new Date(year, current - 1 + delta, 1);
      return { year: date.getFullYear(), month: date.getMonth() + 1 };
    });
  };

  const choose = (date: string, shift: Shift | null) => {
    if (!schedule) return;
    tap();
    const overrides = { ...schedule.overrides };
    // 순서와 같은 근무를 고르면 따로 적어 둘 까닭이 없다.
    const planned = shiftOn({ ...schedule, overrides: {} }, date);
    if (shift === null || shift === planned) delete overrides[date];
    else overrides[date] = shift;
    update({ ...schedule, overrides });
    setPicking(null);
  };

  if (!schedule) {
    return (
      <div className="app center">
        <p className="muted">불러오는 중…</p>
      </div>
    );
  }

  const todayShift = shiftOn(schedule, today);
  const isThisMonth = month.year === new Date().getFullYear() && month.month === new Date().getMonth() + 1;

  return (
    <div className="app">
      <header className="topbar">
        <div className="title">
          <h1>근무표</h1>
          {todayShift && <span className="sync">오늘 {SHIFT_NAME[todayShift]}</span>}
        </div>
        {schedule.anchor && (
          <button className="ghost" onClick={() => (tap(), setSetting(true))}>
            순서 설정
          </button>
        )}
      </header>

      <main className="calendar">
        {setting ? (
          <PatternSetup
            schedule={schedule}
            today={today}
            onSave={(next) => {
              update(next);
              setSetting(false);
            }}
            onCancel={schedule.anchor ? () => setSetting(false) : undefined}
          />
        ) : (
          <>
            <nav className="datebar" aria-label="달">
              <button aria-label="전달" onClick={() => moveMonth(-1)}>
                ‹
              </button>
              <button
                className="date"
                disabled={isThisMonth}
                onClick={() => {
                  const now = new Date();
                  setMonth({ year: now.getFullYear(), month: now.getMonth() + 1 });
                }}
              >
                <strong>
                  {month.year}년 {month.month}월
                </strong>
                {!isThisMonth && <small>이번 달로</small>}
              </button>
              <button aria-label="다음 달" onClick={() => moveMonth(1)}>
                ›
              </button>
            </nav>

            <div className="month" role="grid" aria-label={`${month.year}년 ${month.month}월 근무`}>
              {'일월화수목금토'.split('').map((weekday, index) => (
                <div key={weekday} className={`weekday w${index}`} role="columnheader">
                  {weekday}
                </div>
              ))}
              {cells.map((date, index) => {
                if (!date) return <div key={`blank-${index}`} className="cell blank" />;
                const shift = shiftOn(schedule, date);
                const changed = Boolean(schedule.overrides[date]);
                return (
                  <button
                    key={date}
                    role="gridcell"
                    className={`cell w${index % 7}${date === today ? ' today' : ''}`}
                    aria-label={`${Number(date.slice(8))}일 ${shift ? SHIFT_NAME[shift] : '근무 없음'}${changed ? ' (바꾼 날)' : ''}`}
                    onClick={() => (tap(), setPicking(date))}
                  >
                    <span className="num">{Number(date.slice(8))}</span>
                    {shift && <span className={`chip ${shift}${changed ? ' changed' : ''}`}>{SHIFT_LABEL[shift]}</span>}
                  </button>
                );
              })}
            </div>

            <p className="legend">
              {SHIFTS.map((shift) => (
                <span key={shift}>
                  <span className={`chip ${shift}`}>{SHIFT_LABEL[shift]}</span> {SHIFT_NAME[shift]}
                </span>
              ))}
            </p>
            <p className="muted legend-note">
              순서: {patternText(schedule.pattern)} · 날짜를 누르면 그날만 바꿀 수 있습니다. 점이 찍힌 날은 바꾼 날입니다.
            </p>
          </>
        )}
      </main>

      {picking && (
        <div className="sheet-wrap" onClick={() => setPicking(null)}>
          <section
            className="sheet"
            role="dialog"
            aria-modal="true"
            aria-label="그날 근무 바꾸기"
            onClick={(event) => event.stopPropagation()}
          >
            <div className="grip" aria-hidden="true" />
            <h2 className="sheet-title">
              {Number(picking.slice(5, 7))}월 {Number(picking.slice(8))}일 근무
            </h2>
            <div className="shift-picks">
              {SHIFTS.map((shift) => (
                <button
                  key={shift}
                  className={`shift-pick ${shift}${shiftOn(schedule, picking) === shift ? ' on' : ''}`}
                  onClick={() => choose(picking, shift)}
                >
                  <b>{SHIFT_LABEL[shift]}</b>
                  {SHIFT_NAME[shift]}
                </button>
              ))}
            </div>
            {schedule.overrides[picking] && (
              <button className="secondary wide" onClick={() => choose(picking, null)}>
                순서대로 되돌리기
              </button>
            )}
          </section>
        </div>
      )}

      <TabBar current="calendar" />
    </div>
  );
}

/**
 * 순서 설정: 어떤 순서로 도는지, 오늘이 그 순서의 몇 번째인지.
 */
function PatternSetup({
  schedule,
  today,
  onSave,
  onCancel,
}: {
  schedule: Schedule;
  today: string;
  onSave: (schedule: Schedule) => void;
  onCancel?: () => void;
}) {
  const [text, setText] = useState(patternText(schedule.pattern));
  const pattern = parsePattern(text);
  // 이미 설정돼 있으면 오늘의 자리를 미리 골라 둔다.
  const [position, setPosition] = useState<number | null>(() => {
    if (!schedule.anchor) return null;
    const length = schedule.pattern.length;
    const [y1, m1, d1] = schedule.anchor.split('-').map(Number);
    const [y2, m2, d2] = today.split('-').map(Number);
    const days = Math.round((Date.UTC(y2, m2 - 1, d2) - Date.UTC(y1, m1 - 1, d1)) / 86_400_000);
    return ((days % length) + length) % length;
  });

  const valid = pattern.length > 0 && position !== null && position < pattern.length;

  return (
    <section className="panel">
      <h2>근무 순서</h2>
      <label className="field">
        <span>순서 (주·야·비·휴 글자로)</span>
        <input
          value={text}
          placeholder="주야비휴"
          onChange={(event) => {
            setText(event.target.value);
            setPosition(null);
          }}
        />
      </label>
      <div className="presets">
        {PRESETS.map((preset) => (
          <button
            key={preset}
            className={text === preset ? 'secondary on' : 'secondary'}
            onClick={() => {
              tap();
              setText(preset);
              setPosition(null);
            }}
          >
            {preset}
          </button>
        ))}
      </div>

      <h2>오늘 근무는?</h2>
      {pattern.length === 0 ? (
        <p className="muted">순서를 먼저 적으세요.</p>
      ) : (
        <div className="shift-picks">
          {pattern.map((shift, index) => (
            <button
              key={index}
              className={`shift-pick ${shift}${position === index ? ' on' : ''}`}
              onClick={() => (tap(), setPosition(index))}
            >
              <b>{SHIFT_LABEL[shift]}</b>
              {index + 1}번째
            </button>
          ))}
        </div>
      )}
      <p className="muted">같은 글자가 여럿이면(주주야야…) 오늘이 그중 몇 번째인지 고르세요.</p>

      <button
        className="primary wide"
        disabled={!valid}
        onClick={() => {
          if (!valid) return;
          tap();
          // 순서가 바뀌면 예전에 바꿔 둔 날은 그대로 둔다 — 대근·연차는 순서와 상관없다.
          onSave({ ...schedule, pattern, anchor: anchorFor(today, position!) });
        }}
      >
        저장
      </button>
      {onCancel && (
        <button className="secondary wide" onClick={onCancel}>
          취소
        </button>
      )}
    </section>
  );
}
