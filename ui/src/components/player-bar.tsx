'use client';

import Link from '@/compat/link';
import { useEngineState, usePlayer, useQueue, useUi } from '@/lib/player/context';
import { Progress, ProgressLine, PlayPause, Transport, Volume } from './controls';
import { Cover } from './cover';
import { ListMusic, Maximize2, Monitor, SkipForward } from './icons';
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
function ZoneButton() {
  const { dsp } = useEngineState();
  const { config } = useDsp();
  const on = config.enabled || config.leveling.mode !== 'off';
  return (
    <Link
      href="/dsp"
      className={`zone-btn${dsp ? ' is-dsp' : ''}`}
      title={on ? '이 브라우저 · DSP 켜짐' : '이 브라우저 · DSP 꺼짐'}
      prefetch={false}
    >
      <Monitor size={18} />
      <span className="zone-name">이 브라우저</span>
      {on && <span className="zone-dsp">DSP</span>}
    </Link>
  );
}
