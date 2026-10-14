'use client';

import { usePlayer } from '@/lib/player/context';
import type { PlayableTrack } from '@/lib/player/types';
import { ListEnd, ListPlus, Play, Shuffle } from './icons';

export function PlayButtons({ tracks }: { tracks: PlayableTrack[] }) {
  const { playTracks, playNext, enqueue } = usePlayer();
  const empty = tracks.length === 0;
  return (
    <div className="actions">
      <button type="button" className="btn btn-primary" disabled={empty} onClick={() => playTracks(tracks, 0, { shuffle: false })}>
        <Play size={18} fill="currentColor" /> 재생
      </button>
      <button type="button" className="btn" disabled={empty || tracks.length < 2} onClick={() => playTracks(tracks, 0, { shuffle: true })}>
        <Shuffle size={18} /> 셔플
      </button>
      <button
        type="button"
        className="icon-btn icon-btn-lg"
        disabled={empty}
        onClick={() => playNext(tracks)}
        aria-label="다음에 재생"
        title="다음에 재생"
      >
        <ListPlus size={20} />
      </button>
      <button
        type="button"
        className="icon-btn icon-btn-lg"
        disabled={empty}
        onClick={() => enqueue(tracks)}
        aria-label="대기열 끝에 추가"
        title="대기열 끝에 추가"
      >
        <ListEnd size={20} />
      </button>
    </div>
  );
}
