import Link from 'next/link';
import { AlbumGrid, AlbumRow } from '@/components/album-card';
import { EmptyLibrary } from '@/components/empty-library';
import { Radio, Star } from '@/components/icons';
import { RecentlyPlayed } from '@/components/recently-played';
import { Section } from '@/components/section';
import { config } from '@/lib/config';
import { toCard } from '@/lib/library/build';
import { getScanStatus, loadLibrary } from '@/lib/library/store';
import { tierOf } from '@/lib/quality';
import { count, span } from '@/lib/time';
import { hash } from '@/lib/hash';

/** 날짜가 같으면 같은 순서. 하루 동안은 "오늘의 발견"이 새로고침마다 바뀌지 않는다. */
function dailyPick<T extends { id: string }>(xs: T[], n: number): T[] {
  const day = new Date().toISOString().slice(0, 10);
  return [...xs].sort((a, b) => hash(day + a.id) - hash(day + b.id)).slice(0, n);
}

export default async function Home() {
  const lib = await loadLibrary();
  const s = lib.summary;
  if (!s.tracks) return <EmptyLibrary status={getScanStatus()} musicDir={config.musicDir} />;

  const recent = [...lib.albumList]
    .sort((a, b) => b.addedAt - a.addedAt)
    .slice(0, 18)
    .map(toCard);
  const hires = lib.albumList.filter((a) => tierOf(a.format) === 'hires');
  const picks = dailyPick(lib.albumList, 12).map(toCard);

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
        <AlbumRow albums={recent} />
      </Section>

      {hires.length > 0 && (
        <Section title="Hi-Res" sub={`앨범 ${count(hires.length)}장`} href="/albums?tier=hires">
          <AlbumRow albums={dailyPick(hires, 18).map(toCard)} showYear />
        </Section>
      )}

      <Section title="오늘의 발견" sub="하루에 한 번 바뀝니다">
        <AlbumGrid albums={picks} />
      </Section>

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
