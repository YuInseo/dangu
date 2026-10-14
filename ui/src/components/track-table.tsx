'use client';

import Link from '@/compat/link';
import { Fragment } from 'react';
import { useIsCurrent, usePlayer } from '@/lib/player/context';
import { usePrefs } from '@/lib/prefs';
import type { PlayableTrack } from '@/lib/player/types';
import { formatLabel, tierOf } from '@/lib/quality';
import { clock } from '@/lib/time';
import { Cover } from './cover';
import { TrackMenuButton } from './track-menu';

type Variant = 'album' | 'list' | 'live';

interface Props {
  tracks: PlayableTrack[];
  variant: Variant;
  /** 앨범 아티스트와 다른 곡에만 아티스트를 적는다. */
  albumArtist?: string;
  discs?: boolean;
  /** 앨범 대표 포맷과 다른 곡에만 포맷을 적는다. */
  albumFormat?: string;
}

/** 지금 재생 중이면 움직이는 막대, 멈췄으면 멈춘 막대. */
export function Equalizer({ paused }: { paused?: boolean }) {
  return (
    <span className={`eq${paused ? ' eq-paused' : ''}`} aria-label={paused ? '일시정지됨' : '재생 중'}>
      <span />
      <span />
      <span />
    </span>
  );
}

function Row({
  track,
  index,
  variant,
  albumArtist,
  albumFormat,
  onPlay,
}: {
  track: PlayableTrack;
  index: number;
  variant: Variant;
  albumArtist?: string;
  albumFormat?: string;
  onPlay: (i: number) => void;
}) {
  const state = useIsCurrent(track.key);
  const prefs = usePrefs();
  const showArtist = variant === 'list' || (albumArtist && track.artist !== albumArtist);
  const fmt = formatLabel(track.format);
  const showFmt =
    prefs.formatColumn === 'never' ? false : prefs.formatColumn === 'always' || variant === 'list' || (albumFormat && fmt !== albumFormat);
  const stop = (e: React.MouseEvent) => e.stopPropagation();

  return (
    <div
      className={`trow trow-${variant}${state ? ' is-current' : ''}`}
      role="button"
      tabIndex={0}
      onClick={() => onPlay(index)}
      onKeyDown={(e) => {
        if (e.key === 'Enter' || e.key === ' ') {
          e.preventDefault();
          onPlay(index);
        }
      }}
    >
      <div className="trow-lead">
        {variant === 'list' ? (
          <div className="trow-thumb">
            <Cover src={track.cover?.sm} color={track.cover?.color} alt="" seed={track.art.seed} title={track.art.title} size="sm" />
            {state && (
              <span className="trow-thumb-eq">
                <Equalizer paused={state === 'paused'} />
              </span>
            )}
          </div>
        ) : state ? (
          <Equalizer paused={state === 'paused'} />
        ) : (
          <span className="trow-no">{track.trackNo ?? index + 1}</span>
        )}
      </div>
      <div className="trow-main">
        <div className="trow-title">{track.title}</div>
        {showArtist && (
          <div className="trow-sub">
            {track.artistHref ? (
              <Link href={track.artistHref} onClick={stop} prefetch={false}>
                {track.artist}
              </Link>
            ) : (
              track.artist
            )}
          </div>
        )}
      </div>
      {variant === 'list' && (
        <div className="trow-album">
          {track.albumHref ? (
            <Link href={track.albumHref} onClick={stop} prefetch={false}>
              {track.album}
            </Link>
          ) : (
            track.album
          )}
        </div>
      )}
      <div className={`trow-fmt tier-${tierOf(track.format)}`}>{showFmt ? fmt : ''}</div>
      <div className="trow-time">{track.duration ? clock(track.duration) : ''}</div>
      <div onClick={stop}>
        <TrackMenuButton track={track} />
      </div>
    </div>
  );
}

export function TrackTable({ tracks, variant, albumArtist, discs, albumFormat }: Props) {
  const { playTracks, playNext, enqueue } = usePlayer();
  const { trackClick } = usePrefs();
  const onPlay = (i: number) => {
    const t = tracks[i]!;
    if (trackClick === 'playNow') playTracks([t]);
    else if (trackClick === 'next') playNext([t]);
    else if (trackClick === 'queue') enqueue([t]);
    else playTracks(tracks, i);
  };
  let lastDisc: number | undefined;

  return (
    <div className={`ttable ttable-${variant}`} role="list">
      {tracks.map((t, i) => {
        const disc = t.discNo ?? 1;
        const header = discs && disc !== lastDisc;
        lastDisc = disc;
        return (
          <Fragment key={`${t.key}:${i}`}>
            {header && <div className="disc-head">디스크 {disc}</div>}
            <Row track={t} index={i} variant={variant} albumArtist={albumArtist} albumFormat={albumFormat} onPlay={onPlay} />
          </Fragment>
        );
      })}
    </div>
  );
}
