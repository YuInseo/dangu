'use client';

/**
 * 파라메트릭 EQ 그래프. 점을 끌면 주파수(가로)와 게인(세로)이, 휠을 굴리면 Q가 바뀐다.
 * 빈 곳을 두 번 누르면 그 자리에 밴드가 생긴다.
 *
 * 곡선은 서버의 ffmpeg가 거는 것과 같은 RBJ 공식으로 그린다(테스트가 0.05dB 안에서 맞는지 본다).
 */
import { useMemo, useRef } from 'react';
import { coeffs, hasGain, logFrequencies, magnitudeDb, response } from '@/lib/dsp/biquad';
import type { Band } from '@/lib/dsp/types';

export const BAND_COLORS = ['#e05a5a', '#9b6bff', '#f2c94c', '#ef6aa8', '#7fb8ff', '#5fd3a6', '#ff9f43', '#c39bff', '#4dd0e1', '#a3e635'];

const W = 800;
const H = 300;
const RANGE = 18;
const LO = 20;
const HI = 20000;
const FS = 48000;

const x = (f: number) => (Math.log10(f / LO) / Math.log10(HI / LO)) * W;
const fx = (px: number) => LO * Math.pow(HI / LO, Math.min(1, Math.max(0, px / W)));
const y = (db: number) => H / 2 - (Math.max(-RANGE * 1.5, Math.min(RANGE * 1.5, db)) * (H / 2)) / RANGE;
const gy = (py: number) => ((H / 2 - py) * RANGE) / (H / 2);

const GRID_F = [20, 50, 100, 200, 500, 1000, 2000, 5000, 10000, 20000];
const GRID_DB = [-18, -12, -6, 0, 6, 12, 18];

function label(f: number) {
  return f >= 1000 ? `${f / 1000}k` : String(f);
}

function path(points: [number, number][]) {
  return points.map(([px, py], i) => `${i ? 'L' : 'M'}${px.toFixed(1)},${py.toFixed(1)}`).join(' ');
}

export function EqGraph({
  bands,
  preamp,
  selected,
  onSelect,
  onChange,
  onAdd,
}: {
  bands: Band[];
  preamp: number;
  selected: number | null;
  onSelect: (i: number | null) => void;
  onChange: (i: number, band: Band) => void;
  onAdd: (band: Band) => void;
}) {
  const svg = useRef<SVGSVGElement>(null);
  const drag = useRef<{ i: number; id: number } | null>(null);
  const freqs = useMemo(() => logFrequencies(240, LO, HI), []);

  const total = useMemo(() => response(bands, freqs, FS, preamp), [bands, freqs, preamp]);
  const each = useMemo(
    () =>
      bands.map((b) => {
        if (!b.on) return null;
        const c = coeffs(b, FS);
        return freqs.map((f) => magnitudeDb(c, f, FS));
      }),
    [bands, freqs],
  );

  const toLocal = (e: React.PointerEvent | React.MouseEvent) => {
    const r = svg.current!.getBoundingClientRect();
    return { px: ((e.clientX - r.left) / r.width) * W, py: ((e.clientY - r.top) / r.height) * H };
  };

  const handleY = (b: Band) => (hasGain(b.type) ? y(b.gain) : y(magnitudeDb(coeffs(b, FS), b.freq, FS)));

  return (
    <svg
      ref={svg}
      className="eqgraph"
      viewBox={`0 0 ${W} ${H}`}
      role="img"
      aria-label="EQ 주파수 응답"
      onDoubleClick={(e) => {
        const { px, py } = toLocal(e);
        onAdd({ type: 'peak', freq: Math.round(fx(px)), gain: Math.round(gy(py) * 10) / 10, q: 1, on: true });
      }}
      onPointerDown={(e) => {
        if (e.target === svg.current) onSelect(null);
      }}
      onPointerMove={(e) => {
        const d = drag.current;
        if (!d || d.id !== e.pointerId) return;
        const b = bands[d.i];
        if (!b) return;
        const { px, py } = toLocal(e);
        const freq = Math.round(Math.min(HI, Math.max(LO, fx(px))));
        const gain = hasGain(b.type) ? Math.round(Math.min(RANGE, Math.max(-RANGE, gy(py))) * 10) / 10 : b.gain;
        onChange(d.i, { ...b, freq, gain });
      }}
      onPointerUp={(e) => {
        if (drag.current?.id === e.pointerId) drag.current = null;
      }}
    >
      {/* 눈금 */}
      {GRID_DB.map((db) => (
        <g key={db}>
          <line x1={0} x2={W} y1={y(db)} y2={y(db)} className={db === 0 ? 'eq-zero' : 'eq-grid'} />
          {Math.abs(db) < RANGE && (
            <text x={6} y={y(db) - 4} className="eq-label">
              {db > 0 ? `+${db}` : db}dB
            </text>
          )}
        </g>
      ))}
      {GRID_F.map((f) => (
        <g key={f}>
          <line x1={x(f)} x2={x(f)} y1={0} y2={H} className="eq-grid" />
          <text x={x(f) + 4} y={H - 6} className="eq-label">
            {label(f)}
          </text>
        </g>
      ))}

      {/* 밴드마다 얇은 곡선, 합친 것은 굵게 */}
      {each.map((c, i) =>
        c ? (
          <path
            key={i}
            d={path(c.map((db, k) => [x(freqs[k]!), y(db)]))}
            className="eq-band-curve"
            style={{ stroke: BAND_COLORS[i % BAND_COLORS.length], opacity: selected === i ? 0.95 : 0.5 }}
          />
        ) : null,
      )}
      <path d={path(total.map((db, k) => [x(freqs[k]!), y(db)]))} className="eq-total" />

      {/* 점 */}
      {bands.map((b, i) => (
        <circle
          key={i}
          cx={x(b.freq)}
          cy={handleY(b)}
          r={selected === i ? 9 : 7}
          className={`eq-handle${b.on ? '' : ' is-off'}${selected === i ? ' is-selected' : ''}`}
          style={{ fill: BAND_COLORS[i % BAND_COLORS.length] }}
          tabIndex={0}
          role="slider"
          aria-label={`밴드 ${i + 1}: ${Math.round(b.freq)}Hz ${b.gain.toFixed(1)}dB`}
          aria-valuenow={b.gain}
          onPointerDown={(e) => {
            e.stopPropagation();
            (e.target as Element).setPointerCapture(e.pointerId);
            drag.current = { i, id: e.pointerId };
            onSelect(i);
          }}
          onWheel={(e) => {
            e.preventDefault();
            const q = Math.min(20, Math.max(0.1, b.q * (e.deltaY < 0 ? 1.1 : 1 / 1.1)));
            onChange(i, { ...b, q: Math.round(q * 100) / 100 });
          }}
          onKeyDown={(e) => {
            const step = e.shiftKey ? 1 : 0.1;
            if (e.key === 'ArrowUp' && hasGain(b.type)) onChange(i, { ...b, gain: Math.min(RANGE, Math.round((b.gain + step) * 10) / 10) });
            else if (e.key === 'ArrowDown' && hasGain(b.type))
              onChange(i, { ...b, gain: Math.max(-RANGE, Math.round((b.gain - step) * 10) / 10) });
            else if (e.key === 'ArrowRight') onChange(i, { ...b, freq: Math.min(HI, Math.round(b.freq * (e.shiftKey ? 1.25 : 1.03))) });
            else if (e.key === 'ArrowLeft') onChange(i, { ...b, freq: Math.max(LO, Math.round(b.freq / (e.shiftKey ? 1.25 : 1.03))) });
            else return;
            e.preventDefault();
          }}
        />
      ))}
    </svg>
  );
}
