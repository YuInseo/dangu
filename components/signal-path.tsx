'use client';

import { useEffect, useRef, useState } from 'react';
import { useEngineState, useQueue } from '@/lib/player/context';
import { SIGNAL_LABEL, signalPath, type SignalQuality } from '@/lib/player/signal';
import { formatLabel } from '@/lib/quality';
import { Star } from './icons';

export function QualityGlyph({ quality, size = 12 }: { quality: SignalQuality; size?: number }) {
  if (quality === 'lossless') return <Star className={`glyph q-${quality}`} size={size} fill="currentColor" strokeWidth={0} />;
  return <span className={`glyph-dot q-${quality}`} style={{ width: size * 0.66, height: size * 0.66 }} />;
}

function useSignal() {
  const { current, outputRate, formatOf } = useQueue();
  const s = useEngineState();
  if (!current) return null;
  const format = formatOf(current.track);
  const path = signalPath({
    format,
    source: current.track.source,
    transcoded: s.transcoded,
    volume: s.volume,
    muted: s.muted,
    outputRate,
  });
  return { ...path, format };
}

/** 플레이어의 음질 표시. 누르면 원본부터 출력까지의 단계를 펼친다. */
export function SignalBadge({ large }: { large?: boolean }) {
  const sig = useSignal();
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    const down = (e: PointerEvent) => !ref.current?.contains(e.target as Node) && setOpen(false);
    const key = (e: KeyboardEvent) => e.key === 'Escape' && setOpen(false);
    document.addEventListener('pointerdown', down, true);
    document.addEventListener('keydown', key);
    return () => {
      document.removeEventListener('pointerdown', down, true);
      document.removeEventListener('keydown', key);
    };
  }, [open]);

  if (!sig) return null;
  return (
    <div className={`signal${large ? ' signal-lg' : ''}`} ref={ref}>
      <button
        type="button"
        className={`signal-btn q-${sig.quality}`}
        onClick={() => setOpen((o) => !o)}
        aria-expanded={open}
        title="신호 경로"
      >
        <QualityGlyph quality={sig.quality} size={large ? 14 : 12} />
        <span className="signal-label">{formatLabel(sig.format)}</span>
      </button>
      {open && (
        <div className="signal-pop" role="dialog" aria-label="신호 경로">
          <div className="signal-head">
            <span>신호 경로</span>
            <span className={`signal-q q-${sig.quality}`}>
              <QualityGlyph quality={sig.quality} /> {SIGNAL_LABEL[sig.quality]}
            </span>
          </div>
          <ol className="signal-steps">
            {sig.steps.map((s, i) => (
              <li key={i} className={`signal-step q-${s.quality}`}>
                <span className="signal-node">
                  <QualityGlyph quality={s.quality} size={11} />
                </span>
                <div>
                  <div className="signal-title">{s.title}</div>
                  <div className="signal-detail">{s.detail}</div>
                </div>
              </li>
            ))}
          </ol>
        </div>
      )}
    </div>
  );
}
