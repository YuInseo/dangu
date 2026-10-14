import Link from '@/compat/link';
import { useApi, useTitle } from '@/api';
import { AlbumGrid, AlbumRow } from '@/components/album-card';
import { EmptyLibrary } from '@/components/empty-library';
import { Radio, Star } from '@/components/icons';
import { PrefGate } from '@/components/prefs-client';
import { RecentlyPlayed } from '@/components/recently-played';
import { Section } from '@/components/section';
import type { AlbumCard, LibrarySummary } from '@/lib/library/types';
import { count, span } from '@/lib/time';
import { useLibraryInfo } from '@/library';
import { Failed, Loading } from './common';

interface HomeData {
  summary: LibrarySummary;
  recent: AlbumCard[];
  hires: AlbumCard[];
  hiresCount: number;
  picks: AlbumCard[];
}

export function Home() {
  useTitle(undefined);
  const info = useLibraryInfo();
  const { data, error } = useApi<HomeData>('/api/home');
  if (!info.summary.tracks) return <EmptyLibrary status={info.status} musicDir={info.musicDir} />;
  if (error) return <Failed error={error} />;
  if (!data) return <Loading />;
  const s = data.summary;

  return (
    <div className="home">
      <header className="home-head">
        <div className="eyebrow">내 라이브러리</div>
        <h1 className="page-title">원본 그대로, 지금 들을 음악</h1>
        <div className="stats">
          <Link href="/albums" className="stat">
            <span className="stat-n">{count(s.albums)}</span>
            <span className="stat-l">앨범</span>
          </Link>
          <Link href="/artists" className="stat">
            <span className="stat-n">{count(s.artists)}</span>
            <span className="stat-l">아티스트</span>
          </Link>
          <Link href="/tracks" className="stat">
            <span className="stat-n">{count(s.tracks)}</span>
            <span className="stat-l">곡</span>
          </Link>
          <div className="stat">
            <span className="stat-n">{span(s.duration)}</span>
            <span className="stat-l">전부 듣는 데</span>
          </div>
          <Link href="/albums?tier=lossless" className="stat stat-q">
            <span className="stat-n">
              {s.albums ? Math.round((s.lossless / s.albums) * 100) : 0}
              <small>%</small>
            </span>
            <span className="stat-l">
              <Star size={11} fill="currentColor" strokeWidth={0} /> 무손실 앨범
            </span>
          </Link>
        </div>
      </header>

      <RecentlyPlayed />

      <Section title="최근 추가한 앨범" href="/albums?sort=added">
        <AlbumRow albums={data.recent} />
      </Section>

      {data.hiresCount > 0 && (
        <Section title="Hi-Res" sub={`앨범 ${count(data.hiresCount)}장`} href="/albums?tier=hires">
          <AlbumRow albums={data.hires} showYear />
        </Section>
      )}

      <PrefGate pref="homeDiscover">
        <Section title="오늘의 발견" sub="하루에 한 번 바뀝니다">
          <AlbumGrid albums={data.picks} />
        </Section>
      </PrefGate>

      <Link href="/live" className="live-teaser">
        <div className="live-teaser-icon">
          <Radio size={22} />
        </div>
        <div>
          <div className="live-teaser-title">라이브 아카이브</div>
          <div className="live-teaser-sub">아티스트가 녹음과 공유를 허락한 공연 실황을 무손실 FLAC으로. 무료, 합법, 광고 없음.</div>
        </div>
      </Link>
    </div>
  );
}
