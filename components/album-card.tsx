'use client';

import Link from 'next/link';
import { useState } from 'react';
import type { AlbumCard as AlbumCardData, Track } from '@/lib/library/types';
import { usePlayer } from '@/lib/player/context';
import { usePrefs } from '@/lib/prefs';
import { coverUrls, fromLibrary } from '@/lib/player/tracks';
import { Cover } from './cover';
import { LoaderCircle, Play } from './icons';
import { HiResMark } from './quality-badge';
import { useToast } from './toast';

export function AlbumCard({ album, showYear }: { album: AlbumCardData; showYear?: boolean }) {
  const { playTracks } = usePlayer();
  const prefs = usePrefs();
  const toast = useToast();
  const [busy, setBusy] = useState(false);
  const cover = coverUrls(album.id, album.cover);

  async function play(e: React.MouseEvent) {
    e.preventDefault();
    e.stopPropagation();
    if (busy) return;
    setBusy(true);
    try {
      const res = await fetch(`/api/albums/${album.id}`);
      if (!res.ok) throw new Error();
      const { tracks } = (await res.json()) as { tracks: Track[] };
      playTracks(tracks.map(fromLibrary), 0, { shuffle: prefs.cardPlay === 'shuffle' });
    } catch {
      toast('앨범을 불러오지 못했습니다', 'error');
    } finally {
      setBusy(false);
    }
  }

  return (
    <Link href={`/albums/${album.id}`} className="card" prefetch={false}>
      <div className="card-art">
        <Cover src={cover?.sm} color={cover?.color} alt="" seed={album.id} title={album.title} subtitle={album.artist} />
        {prefs.hiresMark && <HiResMark format={album.format} />}
        <button
          type="button"
          className="card-play"
          onClick={play}
          aria-label={`${album.title} ${prefs.cardPlay === 'shuffle' ? '셔플 ' : ''}재생`}
        >
          {busy ? <LoaderCircle className="spin" size={20} /> : <Play size={20} fill="currentColor" />}
        </button>
      </div>
      <div className="card-title" title={album.title}>
        {album.title}
      </div>
      <div className="card-sub">
        {album.artist}
        {showYear && album.year ? ` · ${album.year}` : ''}
      </div>
    </Link>
  );
}

export function AlbumGrid({ albums, showYear }: { albums: AlbumCardData[]; showYear?: boolean }) {
  return (
    <div className="grid">
      {albums.map((a) => (
        <AlbumCard key={a.id} album={a} showYear={showYear} />
      ))}
    </div>
  );
}

/** 가로로 넘기는 한 줄. 홈 화면의 선반. */
export function AlbumRow({ albums, showYear }: { albums: AlbumCardData[]; showYear?: boolean }) {
  return (
    <div className="row-scroll">
      {albums.map((a) => (
        <AlbumCard key={a.id} album={a} showYear={showYear} />
      ))}
    </div>
  );
}
