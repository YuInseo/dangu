'use client';

import Link from '@/compat/link';
import { usePathname } from '@/compat/navigation';
import { useEffect } from 'react';
import { libraryId, openPlaylistPicker } from '@/lib/collections';
import { useQueue, useUi } from '@/lib/player/context';
import { Progress, Transport, Volume } from './controls';
import { Cover } from './cover';
import { ChevronDown, CirclePlus, ListMusic } from './icons';
import { LikeButton } from './like-button';
import { ZoneButton } from './player-bar';
import { SignalBadge } from './signal-path';

/** 화면을 가득 채우는 지금 재생 중. 표지를 흐리게 깔고 그 위에 크게. */
export function NowPlaying() {
  const { nowPlaying, setNowPlaying, setQueue } = useUi();
  const { current, items, index } = useQueue();
  const pathname = usePathname();

  useEffect(() => {
    setNowPlaying(false);
  }, [pathname, setNowPlaying]);

  useEffect(() => {
    if (!nowPlaying) return;
    const key = (e: KeyboardEvent) => e.key === 'Escape' && setNowPlaying(false);
    document.addEventListener('keydown', key);
    document.documentElement.classList.add('no-scroll');
    return () => {
      document.removeEventListener('keydown', key);
      document.documentElement.classList.remove('no-scroll');
    };
  }, [nowPlaying, setNowPlaying]);

  if (!nowPlaying || !current) return null;
  const t = current.track;
  const id = libraryId(t);
  const upNext = items[index + 1]?.track;

  return (
    <div className="np" role="dialog" aria-modal="true" aria-label="지금 재생 중">
      <div className="np-bg" style={{ backgroundColor: t.cover?.color }}>
        {t.cover && <img src={t.cover.lg} alt="" />}
      </div>
      <div className="np-top">
        <button type="button" className="icon-btn" onClick={() => setNowPlaying(false)} aria-label="닫기">
          <ChevronDown size={26} />
        </button>
        <span className="np-label">지금 재생 중</span>
        <button
          type="button"
          className="icon-btn"
          onClick={() => {
            setNowPlaying(false);
            setQueue(true);
          }}
          aria-label="대기열"
        >
          <ListMusic size={22} />
        </button>
      </div>

      <div className="np-body">
        <div className="np-art">
          <Cover
            src={t.cover?.lg}
            color={t.cover?.color}
            alt={t.album}
            seed={t.art.seed}
            title={t.art.title}
            subtitle={t.art.subtitle}
            size="lg"
            priority
          />
        </div>
        <div className="np-info">
          <div className="np-title-row">
            <div className="np-title">{t.title}</div>
            {id && (
              <div className="np-title-actions">
                <LikeButton kind="tracks" id={id} size={24} name={t.title} />
                <button
                  type="button"
                  className="icon-btn"
                  onClick={() => openPlaylistPicker({ ids: [id], label: `${t.title} — ${t.artist}` })}
                  aria-label="플레이리스트에 추가"
                  title="플레이리스트에 추가"
                >
                  <CirclePlus size={24} />
                </button>
              </div>
            )}
          </div>
          <div className="np-meta">
            {t.artistHref ? <Link href={t.artistHref}>{t.artist}</Link> : t.artist}
            <span className="dot">·</span>
            {t.albumHref ? <Link href={t.albumHref}>{t.album}</Link> : t.album}
            {t.year ? (
              <>
                <span className="dot">·</span>
                {t.year}
              </>
            ) : null}
          </div>
          <div className="np-signal">
            <SignalBadge large />
          </div>
          <Progress className="progress-lg" />
          <Transport size="lg" />
          <div className="np-bottom">
            <Volume />
            <ZoneButton />
            {upNext && (
              <div className="np-next">
                <span className="np-next-label">다음 곡</span>
                <span className="np-next-title">
                  {upNext.title} <span className="muted">— {upNext.artist}</span>
                </span>
              </div>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}
