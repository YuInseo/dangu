'use client';

import { useEffect, useState, type ReactNode } from 'react';

/** 룬의 작은 토글. 켜면 보라, 옆에 켜짐/꺼짐. */
export function Switch({
  on,
  onChange,
  label,
  showText = true,
  disabled,
}: {
  on: boolean;
  onChange: (v: boolean) => void;
  label: string;
  showText?: boolean;
  disabled?: boolean;
}) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={on}
      aria-label={label}
      disabled={disabled}
      className={`switch${on ? ' is-on' : ''}`}
      onClick={(e) => {
        e.stopPropagation();
        onChange(!on);
      }}
    >
      <span className="switch-track">
        <span className="switch-thumb" />
      </span>
      {showText && <span className="switch-text">{on ? '켜짐' : '꺼짐'}</span>}
    </button>
  );
}

/** 숫자 입력. 고치는 동안은 글자 그대로 두고, 벗어나거나 Enter를 누르면 범위 안으로 넣어 반영한다. */
export function NumberField({
  value,
  onChange,
  min,
  max,
  step = 0.1,
  unit,
  label,
  digits = 1,
  width,
}: {
  value: number;
  onChange: (v: number) => void;
  min: number;
  max: number;
  step?: number;
  unit?: string;
  label: string;
  digits?: number;
  width?: number;
}) {
  const [text, setText] = useState(value.toFixed(digits));
  const [editing, setEditing] = useState(false);
  useEffect(() => {
    if (!editing) setText(value.toFixed(digits));
  }, [value, digits, editing]);
  const commit = () => {
    setEditing(false);
    const n = Number(text.replace(',', '.').replace('−', '-'));
    if (!Number.isFinite(n)) {
      setText(value.toFixed(digits));
      return;
    }
    const v = Math.min(max, Math.max(min, n));
    onChange(Math.round(v / step) * step);
    setText(v.toFixed(digits));
  };
  return (
    <label className="numfield" style={width ? { width } : undefined}>
      <input
        inputMode="decimal"
        aria-label={label}
        value={text}
        onFocus={() => setEditing(true)}
        onChange={(e) => setText(e.target.value)}
        onBlur={commit}
        onKeyDown={(e) => {
          if (e.key === 'Enter') (e.target as HTMLInputElement).blur();
          if (e.key === 'ArrowUp' || e.key === 'ArrowDown') {
            e.preventDefault();
            const v = Math.min(max, Math.max(min, value + (e.key === 'ArrowUp' ? step : -step) * (e.shiftKey ? 10 : 1)));
            onChange(Math.round(v / step) * step);
          }
        }}
      />
      {unit && <span className="numfield-unit">{unit}</span>}
    </label>
  );
}

export function Seg<T extends string | number>({
  value,
  options,
  onChange,
  label,
}: {
  value: T;
  options: { value: T; label: ReactNode }[];
  onChange: (v: T) => void;
  label: string;
}) {
  return (
    <div className="seg seg-sm" role="radiogroup" aria-label={label}>
      {options.map((o) => (
        <button
          key={String(o.value)}
          type="button"
          role="radio"
          aria-checked={o.value === value}
          className={`seg-item${o.value === value ? ' is-active' : ''}`}
          onClick={() => onChange(o.value)}
        >
          {o.label}
        </button>
      ))}
    </div>
  );
}

/** 이름 + 설명 + 오른쪽에 조작. */
export function Field({ label, hint, children }: { label: ReactNode; hint?: ReactNode; children: ReactNode }) {
  return (
    <div className="dfield">
      <div className="dfield-text">
        <div className="dfield-label">{label}</div>
        {hint && <div className="dfield-hint">{hint}</div>}
      </div>
      <div className="dfield-control">{children}</div>
    </div>
  );
}

/** 슬라이더와 숫자 칸을 나란히. */
export function RangeField({
  value,
  onChange,
  min,
  max,
  step,
  unit,
  label,
  digits = 1,
}: {
  value: number;
  onChange: (v: number) => void;
  min: number;
  max: number;
  step: number;
  unit: string;
  label: string;
  digits?: number;
}) {
  const pct = ((value - min) / (max - min)) * 100;
  return (
    <div className="rangefield">
      <input
        type="range"
        className="slider"
        min={min}
        max={max}
        step={step}
        value={value}
        aria-label={label}
        style={{ '--pct': `${pct}%` } as React.CSSProperties}
        onChange={(e) => onChange(Number(e.target.value))}
      />
      <NumberField value={value} onChange={onChange} min={min} max={max} step={step} unit={unit} label={label} digits={digits} width={96} />
    </div>
  );
}
