'use client';

import { useEngineState, usePlaybackDuration, usePlaybackTime, usePlayer, useQueue } from '@/lib/player/context';
import { clock } from '@/lib/time';
import { useState } from 'react';
import { LoaderCircle, Pause, Play, Repeat, Repeat1, Shuffle, SkipBack, SkipForward, Volume1, Volume2, VolumeX } from './icons';
import { Slider } from './slider';

export function PlayPause({ size = 'md' }: { size?: 'md' | 'lg' | 'sm' }) {
  const { togglePlay } = usePlayer();
  const { status } = useEngineState();
  const { current } = useQueue();
  const px = size === 'lg' ? 30 : size === 'sm' ? 18 : 20;
  const playing = status === 'playing';
  return (
    <button
      type="button"
      className={`play-btn play-btn-${size}`}
      onClick={togglePlay}
      disabled={!current}
      aria-label={playing ? '일시정지' : '재생'}
    >
      {status === 'loading' ? (
        <LoaderCircle size={px} className="spin" />
      ) : playing ? (
        <Pause size={px} fill="currentColor" strokeWidth={0} />
      ) : (
        <Play size={px} fill="currentColor" strokeWidth={0} className="nudge" />
      )}
    </button>
  );
}

export function Transport({ size = 'md' }: { size?: 'md' | 'lg' }) {
  const { prev, next, toggleShuffle, cycleRepeat } = usePlayer();
  const { shuffle, repeat, current } = useQueue();
  const px = size === 'lg' ? 26 : 19;
  return (
    <div className={`transport transport-${size}`}>
      <button
        type="button"
        className={`icon-btn${shuffle ? ' is-on' : ''}`}
        onClick={toggleShuffle}
        aria-pressed={shuffle}
        aria-label="셔플"
        title="셔플"
      >
        <Shuffle size={px - 3} />
      </button>
      <button type="button" className="icon-btn" onClick={prev} disabled={!current} aria-label="이전 곡">
        <SkipBack size={px} fill="currentColor" />
      </button>
      <PlayPause size={size} />
      <button type="button" className="icon-btn" onClick={next} disabled={!current} aria-label="다음 곡">
        <SkipForward size={px} fill="currentColor" />
      </button>
      <button
        type="button"
        className={`icon-btn${repeat !== 'off' ? ' is-on' : ''}`}
        onClick={cycleRepeat}
        aria-label={repeat === 'one' ? '한 곡 반복' : repeat === 'all' ? '전체 반복' : '반복 안 함'}
        title={repeat === 'one' ? '한 곡 반복' : repeat === 'all' ? '전체 반복' : '반복 안 함'}
      >
        {repeat === 'one' ? <Repeat1 size={px - 3} /> : <Repeat size={px - 3} />}
      </button>
    </div>
  );
}

export function Progress({ className }: { className?: string }) {
  const { seek } = usePlayer();
  const t = usePlaybackTime();
  const d = usePlaybackDuration();
  const [scrub, setScrub] = useState<number | null>(null);
  return (
    <div className={`progress${className ? ` ${className}` : ''}`}>
      <span className="progress-time">{clock(scrub ?? t)}</span>
      <Slider value={t} max={d} onCommit={seek} onScrub={setScrub} label="재생 위치" className="slider-progress" />
      <span className="progress-time">{clock(d)}</span>
    </div>
  );
}

/** 모바일 미니 플레이어 위의 얇은 선. 누를 수는 없다. */
export function ProgressLine() {
  const t = usePlaybackTime();
  const d = usePlaybackDuration();
  return (
    <div className="progress-line" aria-hidden>
      <span style={{ width: `${d ? Math.min(100, (t / d) * 100) : 0}%` }} />
    </div>
  );
}

export function Volume() {
  const { setVolume, toggleMute } = usePlayer();
  const { volume, muted } = useEngineState();
  const Icon = muted || volume === 0 ? VolumeX : volume < 0.5 ? Volume1 : Volume2;
  return (
    <div className="volume">
      <button type="button" className="icon-btn" onClick={toggleMute} aria-label={muted ? '음소거 해제' : '음소거'}>
        <Icon size={19} />
      </button>
      <Slider value={muted ? 0 : volume} max={1} onCommit={setVolume} live label="볼륨" className="slider-volume" />
    </div>
  );
}
