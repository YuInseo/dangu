'use client';

import Link from 'next/link';
import { useEffect, useLayoutEffect, useRef, useState } from 'react';
import { createPortal } from 'react-dom';
import { usePlayer } from '@/lib/player/context';
import type { PlayableTrack } from '@/lib/player/types';
import { Disc3, Ellipsis, ListEnd, ListPlus, MicVocal } from './icons';

/** 곡 옆의 "⋯". 다음에 재생, 대기열에 추가, 앨범·아티스트로 가기. */
export function TrackMenuButton({ track, label = '더 보기' }: { track: PlayableTrack; label?: string }) {
  const [anchor, setAnchor] = useState<DOMRect | null>(null);
  return (
    <>
      <button
        type="button"
        className="icon-btn trow-menu"
        aria-label={label}
        aria-haspopup="menu"
        onClick={(e) => {
          e.stopPropagation();
          setAnchor(anchor ? null : e.currentTarget.getBoundingClientRect());
        }}
      >
        <Ellipsis size={18} />
      </button>
      {anchor && <TrackMenu track={track} anchor={anchor} onClose={() => setAnchor(null)} />}
    </>
  );
}

function TrackMenu({ track, anchor, onClose }: { track: PlayableTrack; anchor: DOMRect; onClose: () => void }) {
  const { playNext, enqueue } = usePlayer();
  const ref = useRef<HTMLDivElement>(null);
  const [pos, setPos] = useState({ top: anchor.bottom + 4, left: anchor.right - 220 });

  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return;
    const h = el.offsetHeight;
    const w = el.offsetWidth;
    const top = anchor.bottom + h + 8 > window.innerHeight ? anchor.top - h - 4 : anchor.bottom + 4;
    const left = Math.min(Math.max(8, anchor.right - w), window.innerWidth - w - 8);
    setPos({ top, left });
  }, [anchor]);

  useEffect(() => {
    const down = (e: PointerEvent) => {
      if (!ref.current?.contains(e.target as Node)) onClose();
    };
    const key = (e: KeyboardEvent) => e.key === 'Escape' && onClose();
    document.addEventListener('pointerdown', down, true);
    document.addEventListener('keydown', key);
    window.addEventListener('scroll', onClose, true);
    window.addEventListener('resize', onClose);
    ref.current?.querySelector<HTMLElement>('[role=menuitem]')?.focus();
    return () => {
      document.removeEventListener('pointerdown', down, true);
      document.removeEventListener('keydown', key);
      window.removeEventListener('scroll', onClose, true);
      window.removeEventListener('resize', onClose);
    };
  }, [onClose]);

  const act = (fn: () => void) => (e: React.MouseEvent) => {
    e.stopPropagation();
    fn();
    onClose();
  };

  return createPortal(
    <div ref={ref} className="menu" role="menu" style={{ top: pos.top, left: pos.left }} onClick={(e) => e.stopPropagation()}>
      <div className="menu-head">
        <div className="menu-title">{track.title}</div>
        <div className="menu-sub">{track.artist}</div>
      </div>
      <button type="button" role="menuitem" className="menu-item" onClick={act(() => playNext([track]))}>
        <ListPlus size={17} /> 다음에 재생
      </button>
      <button type="button" role="menuitem" className="menu-item" onClick={act(() => enqueue([track]))}>
        <ListEnd size={17} /> 대기열 끝에 추가
      </button>
      {track.albumHref && (
        <Link href={track.albumHref} role="menuitem" className="menu-item" onClick={onClose}>
          <Disc3 size={17} /> {track.source === 'live' ? '공연으로 이동' : '앨범으로 이동'}
        </Link>
      )}
      {track.artistHref && (
        <Link href={track.artistHref} role="menuitem" className="menu-item" onClick={onClose}>
          <MicVocal size={17} /> 아티스트로 이동
        </Link>
      )}
    </div>,
    document.body,
  );
}
