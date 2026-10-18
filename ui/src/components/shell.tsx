'use client';

import { Suspense, useEffect, type ReactNode } from 'react';
import { ArrowLeft, Link2 } from '@/components/icons';
import type { LibrarySummary, ScanStatus } from '@/lib/library/types';
import { forgetVia, viaHome } from '@/lib/link';
import { useExternalLinks } from '@/lib/external';
import { useLibraryInfo } from '@/library';
import { usePlayer, useUi } from '@/lib/player/context';
import { NowPlaying } from './now-playing';
import { PlayerBar } from './player-bar';
import { PlaylistPicker } from './playlist-picker';
import { QueuePanel } from './queue-panel';
import { MobileNav, Sidebar } from './sidebar';
import { Topbar } from './topbar';
import { UpdateBanner } from './update';

function typing(target: EventTarget | null) {
  const el = target as HTMLElement | null;
  if (!el) return false;
  const tag = el.tagName;
  return tag === 'INPUT' || tag === 'TEXTAREA' || tag === 'SELECT' || el.isContentEditable;
}

/**
 * 단축키.
 *   Space 재생/일시정지 · ←/→ 10초 · Shift+←/→ 이전/다음 곡 · M 음소거
 *   / 검색 · Q 대기열 · F 지금 재생 중
 */
function useShortcuts() {
  const p = usePlayer();
  const ui = useUi();
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.metaKey || e.ctrlKey || e.altKey) {
        if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === 'k') {
          e.preventDefault();
          window.dispatchEvent(new Event('sori:focus-search'));
        }
        return;
      }
      if (typing(e.target)) return;
      const onControl = (e.target as HTMLElement | null)?.closest?.('button, a, [role=button], [role=menuitem]');
      switch (e.key) {
        case ' ':
          if (onControl) return;
          e.preventDefault();
          p.togglePlay();
          break;
        case 'ArrowLeft':
          e.preventDefault();
          if (e.shiftKey) p.prev();
          else p.seek(p.engine.currentTime - 10);
          break;
        case 'ArrowRight':
          e.preventDefault();
          if (e.shiftKey) p.next();
          else p.seek(p.engine.currentTime + 10);
          break;
        case 'm':
        case 'M':
          p.toggleMute();
          break;
        case '/':
          e.preventDefault();
          window.dispatchEvent(new Event('sori:focus-search'));
          break;
        case 'q':
        case 'Q':
          ui.setQueue(!ui.queue);
          break;
        case 'f':
        case 'F':
          ui.setNowPlaying(!ui.nowPlaying);
          break;
      }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [p, ui]);
}

/** 다른 기기의 소리에 연결해 왔으면, 지금 어디에 있는지와 돌아가는 길. */
function ViaBar() {
  const info = useLibraryInfo();
  const home = viaHome();
  if (!home) return null;
  return (
    <div className="via-bar" role="status">
      <Link2 size={15} />
      <span className="via-bar-text">
        <strong>{info.deviceName ?? '다른 기기'}</strong>의 소리에 연결됨
      </span>
      <a
        className="via-bar-back"
        href={home}
        onClick={() => {
          forgetVia();
        }}
      >
        <ArrowLeft size={15} /> 이 기기로
      </a>
    </div>
  );
}

export function Shell({ children, status, summary }: { children: ReactNode; status: ScanStatus; summary: LibrarySummary }) {
  useShortcuts();
  useExternalLinks(!!useLibraryInfo().host);
  return (
    <div className="app">
      <Sidebar status={status} summary={summary} />
      <main className="main">
        <ViaBar />
        <Suspense fallback={<div className="topbar" />}>
          <Topbar />
        </Suspense>
        <div className="page">{children}</div>
      </main>
      <QueuePanel />
      <PlayerBar />
      <NowPlaying />
      <MobileNav />
      <PlaylistPicker />
      <UpdateBanner />
    </div>
  );
}
