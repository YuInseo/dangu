'use client';

import { useEffect, useRef, useState } from 'react';
import Link from '@/compat/link';
import { useEngineState, usePlayer, useQueue, useUi, useZone } from '@/lib/player/context';
import { Progress, ProgressLine, PlayPause, Transport, Volume } from './controls';
import { Cover } from './cover';
import { Check, ListMusic, Maximize2, Monitor, SkipForward, Speaker } from './icons';
import { useDsp } from '@/lib/dsp/store';
import { SignalBadge } from './signal-path';

export function PlayerBar() {
  const { current } = useQueue();
  const { setNowPlaying, queue, setQueue } = useUi();
  const { next } = usePlayer();
  const t = current?.track;

  return (
    <footer className={`player${t ? '' : ' is-empty'}`}>
      <ProgressLine />
      <div className="player-now">
        {t ? (
          <>
            <button type="button" className="player-art" onClick={() => setNowPlaying(true)} aria-label="지금 재생 중 화면 열기">
              <Cover
                src={t.cover?.sm}
                color={t.cover?.color}
                alt=""
                seed={t.art.seed}
                title={t.art.title}
                subtitle={t.art.subtitle}
                size="sm"
              />
            </button>
            <div className="player-text" onClick={() => window.matchMedia('(max-width: 760px)').matches && setNowPlaying(true)}>
              <div className="player-title" title={t.title}>
                {t.albumHref ? (
                  <Link href={t.albumHref} prefetch={false}>
                    {t.title}
                  </Link>
                ) : (
                  t.title
                )}
              </div>
              <div className="player-sub">
                {t.artistHref ? (
                  <Link href={t.artistHref} prefetch={false}>
                    {t.artist}
                  </Link>
                ) : (
                  t.artist
                )}
              </div>
            </div>
          </>
        ) : (
          <div className="player-idle">재생할 곡을 고르세요</div>
        )}
      </div>

      <div className="player-center">
        <Transport />
        <Progress />
      </div>

      <div className="player-right">
        <SignalBadge />
        <button
          type="button"
          className={`icon-btn${queue ? ' is-on' : ''}`}
          onClick={() => setQueue(!queue)}
          aria-label="대기열"
          aria-pressed={queue}
          title="대기열 (Q)"
        >
          <ListMusic size={19} />
        </button>
        <Volume />
        <ZoneButton />
        <button
          type="button"
          className="icon-btn"
          onClick={() => setNowPlaying(true)}
          disabled={!t}
          aria-label="지금 재생 중 화면"
          title="지금 재생 중 (F)"
        >
          <Maximize2 size={17} />
        </button>
      </div>

      <div className="player-mobile">
        <PlayPause size="sm" />
        <button type="button" className="icon-btn" onClick={next} disabled={!t} aria-label="다음 곡">
          <SkipForward size={19} fill="currentColor" />
        </button>
      </div>
    </footer>
  );
}

/** 룬의 존 표시. 이 브라우저가 곧 하나의 존이고, 누르면 이 존의 DSP 설정으로 간다. */
/** 소리를 낼 곳. 이 브라우저, 또는 소리 서버가 도는 기기의 오디오 장치. */
function ZoneButton() {
  const { dsp } = useEngineState();
  const { config } = useDsp();
  const { setZone } = usePlayer();
  const { zone, nativeAvailable } = useZone();
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);
  const on = config.enabled || config.leveling.mode !== 'off';
  const name = zone === 'native' ? '이 기기' : '이 브라우저';

  useEffect(() => {
    if (!open) return;
    const close = (e: PointerEvent) => {
      if (!ref.current?.contains(e.target as Node)) setOpen(false);
    };
    const esc = (e: KeyboardEvent) => e.key === 'Escape' && setOpen(false);
    window.addEventListener('pointerdown', close);
    window.addEventListener('keydown', esc);
    return () => {
      window.removeEventListener('pointerdown', close);
      window.removeEventListener('keydown', esc);
    };
  }, [open]);

  if (!nativeAvailable) {
    return (
      <Link href="/dsp" className={`zone-btn${dsp ? ' is-dsp' : ''}`} title={on ? '이 브라우저 · DSP 켜짐' : '이 브라우저 · DSP 꺼짐'} prefetch={false}>
        <Monitor size={18} />
        <span className="zone-name">이 브라우저</span>
        {on && <span className="zone-dsp">DSP</span>}
      </Link>
    );
  }
  return (
    <div className="zone-wrap" ref={ref}>
      <button type="button" className={`zone-btn${dsp ? ' is-dsp' : ''}`} onClick={() => setOpen(!open)} aria-expanded={open} title={`${name} · 소리 낼 곳 바꾸기`}>
        {zone === 'native' ? <Speaker size={18} /> : <Monitor size={18} />}
        <span className="zone-name">{name}</span>
        {on && <span className="zone-dsp">DSP</span>}
      </button>
      {open && (
        <div className="zone-menu" role="menu">
          <div className="zone-menu-title">소리 낼 곳</div>
          {(
            [
              ['native', '이 기기', '소리 엔진이 오디오 장치로 직접 · 64비트', Speaker],
              ['browser', '이 브라우저', '브라우저가 받아서 재생 · OS 믹서', Monitor],
            ] as const
          ).map(([k, label, hint, Icon]) => (
            <button
              key={k}
              type="button"
              role="menuitemradio"
              aria-checked={zone === k}
              className={`zone-item${zone === k ? ' is-active' : ''}`}
              onClick={() => {
                setZone(k);
                setOpen(false);
              }}
            >
              <Icon size={18} />
              <span className="zone-item-text">
                <span className="zone-item-name">{label}</span>
                <span className="zone-item-hint">{hint}</span>
              </span>
              {zone === k && <Check size={16} className="zone-item-check" />}
            </button>
          ))}
          <Link href="/dsp" className="zone-menu-link" onClick={() => setOpen(false)}>
            DSP 설정
          </Link>
        </div>
      )}
    </div>
  );
}
