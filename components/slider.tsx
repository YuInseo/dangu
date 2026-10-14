'use client';

import { useRef, useState, type CSSProperties } from 'react';

/**
 * 진행 막대와 볼륨. 끄는 동안에는 손가락 위치를 보여 주고, 놓을 때 한 번만 반영한다
 * (`live`면 끄는 내내 반영한다 — 볼륨처럼).
 */
export function Slider({
  value,
  max,
  onCommit,
  label,
  className,
  live,
  onScrub,
}: {
  value: number;
  max: number;
  onCommit: (v: number) => void;
  label: string;
  className?: string;
  live?: boolean;
  onScrub?: (v: number | null) => void;
}) {
  const [drag, setDrag] = useState<number | null>(null);
  const dragging = useRef(false);
  const top = max > 0 ? max : 1;
  const v = Math.min(drag ?? value, top);
  const pct = (v / top) * 100;

  const end = (x: number | null) => {
    if (!dragging.current) return;
    dragging.current = false;
    if (x != null) onCommit(x);
    setDrag(null);
    onScrub?.(null);
  };

  return (
    <input
      type="range"
      className={`slider${className ? ` ${className}` : ''}`}
      min={0}
      max={top}
      step="any"
      value={v}
      aria-label={label}
      style={{ '--pct': `${pct}%` } as CSSProperties}
      onPointerDown={() => {
        dragging.current = true;
      }}
      onChange={(e) => {
        const x = Number(e.target.value);
        if (dragging.current && !live) {
          setDrag(x);
          onScrub?.(x);
        } else onCommit(x);
      }}
      onPointerUp={(e) => end(Number(e.currentTarget.value))}
      onPointerCancel={() => end(null)}
      onLostPointerCapture={(e) => end(Number(e.currentTarget.value))}
    />
  );
}
