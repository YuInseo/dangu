'use client';

import { useEffect, useRef } from 'react';

import { tap } from '../lib/platform';
import { PLATFORMS } from '../lib/slip';

/**
 * 시각 고르기 — 시와 분을 굴려서.
 *
 * 숫자 키패드로 `1713`을 치는 것보다, 승강장에서 한 손으로는 엄지로 굴리는 쪽이 빠르고
 * 틀릴 일이 적다. 콜론을 찾을 필요도, 키보드가 화면 절반을 덮을 일도 없다.
 *
 * 값은 `HH:MM` 문자열 하나로 주고받는다. 쪽지에 저장되는 모양 그대로다.
 */
export function TimeWheel({ value, onChange }: { value: string; onChange: (value: string) => void }) {
  const match = /^(\d{1,2}):(\d{2})$/.exec(value);
  const hour = match ? Math.min(23, Number(match[1])) : 0;
  const minute = match ? Math.min(59, Number(match[2])) : 0;
  const pad = (n: number) => String(n).padStart(2, '0');

  return (
    <div className="time-wheel" role="group" aria-label="시각">
      <Wheel items={HOURS} value={hour} label="시" onChange={(h) => onChange(`${pad(h)}:${pad(minute)}`)} />
      <span className="time-colon" aria-hidden="true">
        :
      </span>
      <Wheel items={MINUTES} value={minute} label="분" onChange={(m) => onChange(`${pad(hour)}:${pad(m)}`)} />
    </div>
  );
}

/**
 * 홈 고르기 — 4번부터 14번까지 굴려서. 맨 위의 "—"는 아직 모른다는 뜻이다.
 *
 * 홈은 열차가 들어오기 직전까지 바뀌곤 해서, 쓰기 화면과 펼친 쪽지 양쪽에서 같은 휠로 고친다.
 */
const PLATFORM_ITEMS = ['—', ...PLATFORMS];

export function PlatformWheel({ value, onChange }: { value: string; onChange: (value: string) => void }) {
  const index = Math.max(0, PLATFORM_ITEMS.indexOf(value || '—'));
  return (
    <div className="time-wheel platform-wheel" role="group" aria-label="홈">
      <Wheel
        items={PLATFORM_ITEMS}
        value={index}
        label="번 홈"
        onChange={(next) => onChange(next === 0 ? '' : PLATFORM_ITEMS[next])}
      />
    </div>
  );
}

const HOURS = Array.from({ length: 24 }, (_, n) => String(n).padStart(2, '0'));
const MINUTES = Array.from({ length: 60 }, (_, n) => String(n).padStart(2, '0'));

/** 한 칸 높이. CSS의 `--wheel-item`과 같아야 한다. */
const ITEM = 44;

/**
 * 굴리는 열 하나.
 *
 * 브라우저의 스크롤 스냅이 칸에 딱 맞춰 멈추게 해 준다. 멈춘 뒤에 위치에서 값을 읽는다 —
 * 굴리는 도중에 값을 바꾸면 화면이 다시 그려지면서 손가락 밑의 목록이 튄다.
 */
export function Wheel({
  items,
  value,
  label,
  onChange,
}: {
  /** 칸마다 보일 글자. `value`는 이 배열의 자리(index)다. */
  items: string[];
  value: number;
  label: string;
  onChange: (value: number) => void;
}) {
  const count = items.length;
  const ref = useRef<HTMLDivElement>(null);
  const settle = useRef<ReturnType<typeof setTimeout> | null>(null);
  const shown = useRef(value);

  // 밖에서 값이 바뀌면(처음 열 때, "지금"을 눌렀을 때) 그 자리로 굴려 둔다.
  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    if (Math.round(el.scrollTop / ITEM) !== value) el.scrollTo({ top: value * ITEM });
    shown.current = value;
  }, [value]);

  useEffect(() => () => {
    if (settle.current) clearTimeout(settle.current);
  }, []);

  const onScroll = () => {
    const el = ref.current;
    if (!el) return;
    const index = Math.max(0, Math.min(count - 1, Math.round(el.scrollTop / ITEM)));
    // 칸을 지날 때마다 손끝에 한 번. 몇 칸 굴렸는지 화면을 안 봐도 안다.
    if (index !== shown.current) {
      shown.current = index;
      tap();
    }
    if (settle.current) clearTimeout(settle.current);
    settle.current = setTimeout(() => {
      if (index !== value) onChange(index);
    }, 120);
  };

  const step = (delta: number) => {
    const next = (value + delta + count) % count;
    tap();
    onChange(next);
  };

  return (
    <div className="wheel">
      <div
        ref={ref}
        className="wheel-list"
        role="listbox"
        aria-label={label}
        tabIndex={0}
        onScroll={onScroll}
        onKeyDown={(event) => {
          if (event.key === 'ArrowUp') {
            event.preventDefault();
            step(-1);
          } else if (event.key === 'ArrowDown') {
            event.preventDefault();
            step(1);
          }
        }}
      >
        <div className="wheel-pad" aria-hidden="true" />
        {items.map((item, n) => (
          <div
            key={n}
            role="option"
            aria-selected={n === value}
            className={n === value ? 'wheel-item on' : 'wheel-item'}
            onClick={() => {
              ref.current?.scrollTo({ top: n * ITEM, behavior: 'smooth' });
            }}
          >
            {item}
          </div>
        ))}
        <div className="wheel-pad" aria-hidden="true" />
      </div>
      <span className="wheel-unit">{label}</span>
    </div>
  );
}
