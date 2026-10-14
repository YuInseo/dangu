'use client';

import Link from '@/compat/link';
import { useEffect, useRef, useState } from 'react';
import { useEngineState, usePlaybackTime, usePlayer, useQueue } from '@/lib/player/context';
import { NativeEngine } from '@/lib/player/native';
import { SIGNAL_LABEL, signalPath, type SignalQuality } from '@/lib/player/signal';
import { formatLabel } from '@/lib/quality';
import { Star } from './icons';

/** 룬의 표시: 무손실·향상은 별(보라·파랑), 고품질·저품질은 점(초록·노랑). */
export function QualityGlyph({ quality, size = 12 }: { quality: SignalQuality; size?: number }) {
  if (quality === 'lossless' || quality === 'enhanced') {
    return <Star className={`glyph q-${quality}`} size={size} fill="currentColor" strokeWidth={0} />;
  }
  return <span className={`glyph-dot q-${quality}`} style={{ width: size * 0.62, height: size * 0.62 }} />;
}

function useSignal() {
  const { current, outputRate, formatOf, caps } = useQueue();
  const { engine } = usePlayer();
  const s = useEngineState();
  if (!current) return null;
  const format = formatOf(current.track);
  const native = engine instanceof NativeEngine ? { output: engine.output, source: engine.source, resampling: engine.resampling } : null;
  const path = signalPath({
    native,
    format,
    source: current.track.source,
    transcoded: s.transcoded,
    dsp: s.dsp,
    loudness: current.track.loudness,
    caps,
    volume: s.volume,
    muted: s.muted,
    outputRate,
  });
  return { ...path, format, dsp: !!s.dsp };
}

/**
 * 클리핑 표시기. 서버가 기록한 "곡의 몇 초에서 넘쳤는지"를 받아 두었다가, 재생 위치가 그 시각을
 * 지나는 순간 켠다. 한 샘플만 넘쳐도 켜진다(룬처럼 최대 감도).
 */
export function useClipIndicator() {
  const { sid, dsp } = useEngineState();
  const t = usePlaybackTime();
  const [times, setTimes] = useState<number[]>([]);
  const [lit, setLit] = useState(false);
  const [count, setCount] = useState(0);
  const last = useRef(0);
  const off = useRef<ReturnType<typeof setTimeout>>(undefined);
  const enabled = !!sid && !!dsp?.ci;

  useEffect(() => {
    setTimes([]);
    setCount(0);
    setLit(false);
    if (!enabled) return;
    let alive = true;
    const poll = async () => {
      try {
        const r = await fetch(`/api/dsp/clips?sid=${encodeURIComponent(sid!)}`, { cache: 'no-store' });
        if (r.ok && alive) setTimes(((await r.json()) as { times: number[] }).times);
      } catch {
        // 다음에
      }
    };
    void poll();
    const timer = setInterval(poll, 1500);
    return () => {
      alive = false;
      clearInterval(timer);
    };
  }, [enabled, sid]);

  useEffect(() => {
    const prev = last.current;
    last.current = t;
    if (!enabled || t < prev || t - prev > 2) return; // 탐색은 건너뛴다
    const hits = times.filter((x) => x > prev && x <= t).length;
    if (!hits) return;
    setCount((c) => c + hits);
    setLit(true);
    clearTimeout(off.current);
    off.current = setTimeout(() => setLit(false), 1500);
  }, [t, times, enabled]);

  return { enabled, lit, count };
}

/** 플레이어의 음질 표시. 누르면 원본부터 출력까지의 단계를 펼친다. */
export function SignalBadge({ large }: { large?: boolean }) {
  const sig = useSignal();
  const clip = useClipIndicator();
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);
  const { engine } = usePlayer();

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
        className={`signal-btn q-${sig.quality}${clip.lit ? ' is-clipping' : ''}`}
        onClick={() => setOpen((o) => !o)}
        aria-expanded={open}
        title="신호 경로"
      >
        <QualityGlyph quality={sig.quality} size={large ? 14 : 12} />
        <span className="signal-label">{formatLabel(sig.format)}</span>
        {sig.dsp && <span className="signal-dsp">DSP</span>}
        {clip.enabled && <span className={`clip-light${clip.lit ? ' is-on' : ''}`} aria-label={clip.lit ? '클리핑' : undefined} />}
      </button>
      {open && (
        <div className="signal-pop" role="dialog" aria-label="신호 경로">
          <div className="signal-head">
            <span>
              신호 경로: <span className={`q-${sig.quality}`}>{SIGNAL_LABEL[sig.quality]}</span>
            </span>
            <Link href="/dsp" className="signal-link" onClick={() => setOpen(false)}>
              DSP 설정
            </Link>
          </div>
          <div className="signal-zone">
            <span className={`glyph-dot q-${sig.quality}`} />{' '}
            {engine instanceof NativeEngine ? `이 기기 · ${engine.output?.device ?? '오디오 장치'}` : `이 브라우저 · 존 ${engine.zone}`}
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
          {clip.enabled && (
            <div className={`signal-clip${clip.count ? ' has-clips' : ''}`}>
              <span className={`clip-light${clip.lit ? ' is-on' : ''}`} />
              {clip.count ? `이 곡에서 클리핑 ${clip.count}번 — 헤드룸을 더 내리세요` : '클리핑 없음'}
            </div>
          )}
        </div>
      )}
    </div>
  );
}
