'use client';

import type { ReactNode } from 'react';
import { libraryIds, openPlaylistPicker } from '@/lib/collections';
import { usePlayer } from '@/lib/player/context';
import type { PlayableTrack } from '@/lib/player/types';
import { CirclePlus, ListEnd, ListPlus, Play, Shuffle } from './icons';
import { LikeButton } from './like-button';

/**
 * 재생 · 셔플 · (앨범이면 ♥) · 다음에 재생 · 대기열 끝에 · 플레이리스트에 추가, 그리고 `children`.
 * `label`은 플레이리스트 창 위에 적을 이름(앨범 제목 같은 것).
 */
export function PlayButtons({ tracks, albumId, label, children }: { tracks: PlayableTrack[]; albumId?: string; label?: string; children?: ReactNode }) {
  const { playTracks, playNext, enqueue } = usePlayer();
  const empty = tracks.length === 0;
  const ids = libraryIds(tracks);
  return (
    <div className="actions play-actions">
      <button type="button" className="btn btn-primary" disabled={empty} onClick={() => playTracks(tracks, 0, { shuffle: false })}>
        <Play size={18} fill="currentColor" /> 재생
      </button>
      <button type="button" className="btn" disabled={empty || tracks.length < 2} onClick={() => playTracks(tracks, 0, { shuffle: true })}>
        <Shuffle size={18} /> 셔플
      </button>
      {albumId && <LikeButton kind="albums" id={albumId} className="icon-btn-lg" name={label} />}
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
      {ids.length > 0 && (
        <button
          type="button"
          className="icon-btn icon-btn-lg"
          onClick={() => openPlaylistPicker({ ids, label: label ? `${label} · ${ids.length}곡` : `${ids.length}곡`, suggest: label })}
          aria-label="플레이리스트에 추가"
          title="플레이리스트에 추가"
        >
          <CirclePlus size={20} />
        </button>
      )}
      {children}
    </div>
  );
}
