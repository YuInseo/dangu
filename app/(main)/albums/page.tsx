import Link from 'next/link';
import type { Metadata } from 'next';
import { AlbumGrid } from '@/components/album-card';
import { EmptyLibrary } from '@/components/empty-library';
import { PageHeader } from '@/components/section';
import { config } from '@/lib/config';
import { collator, compareArtist, sortName, toCard } from '@/lib/library/build';
import { getScanStatus, loadLibrary } from '@/lib/library/store';
import type { Album, QualityTier } from '@/lib/library/types';
import { tierOf } from '@/lib/quality';
import { count } from '@/lib/time';

export const metadata: Metadata = { title: '앨범' };

const SORTS = {
  artist: { label: '아티스트', fn: (a: Album, b: Album) => compareArtist(a.artist, b.artist) || (a.year ?? 0) - (b.year ?? 0) },
  title: { label: '제목', fn: (a: Album, b: Album) => collator.compare(sortName(a.title), sortName(b.title)) },
  year: { label: '연도', fn: (a: Album, b: Album) => (b.year ?? 0) - (a.year ?? 0) || collator.compare(a.title, b.title) },
  added: { label: '최근 추가', fn: (a: Album, b: Album) => b.addedAt - a.addedAt },
} as const;

const TIERS: { key: QualityTier | 'all'; label: string }[] = [
  { key: 'all', label: '전체' },
  { key: 'hires', label: 'Hi-Res' },
  { key: 'lossless', label: '무손실' },
  { key: 'lossy', label: '손실 압축' },
];

type SortKey = keyof typeof SORTS;

export default async function AlbumsPage({ searchParams }: { searchParams: Promise<{ sort?: string; tier?: string }> }) {
  const sp = await searchParams;
  const lib = await loadLibrary();
  if (!lib.summary.tracks) return <EmptyLibrary status={getScanStatus()} musicDir={config.musicDir} />;

  const sort: SortKey = sp.sort && sp.sort in SORTS ? (sp.sort as SortKey) : 'artist';
  const tier = TIERS.some((t) => t.key === sp.tier) ? (sp.tier as QualityTier | 'all') : 'all';

  let albums = lib.albumList;
  // "무손실"은 Hi-Res까지 포함한다. 무손실이 아닌 것만 빼는 필터로 읽히는 게 자연스럽다.
  if (tier === 'hires') albums = albums.filter((a) => tierOf(a.format) === 'hires');
  else if (tier === 'lossless') albums = albums.filter((a) => a.format.lossless);
  else if (tier === 'lossy') albums = albums.filter((a) => !a.format.lossless);
  albums = sort === 'artist' ? albums : [...albums].sort(SORTS[sort].fn);

  const href = (p: { sort?: string; tier?: string }) => {
    const q = new URLSearchParams();
    const s = p.sort ?? sort;
    const t = p.tier ?? tier;
    if (s !== 'artist') q.set('sort', s);
    if (t !== 'all') q.set('tier', t);
    const str = q.toString();
    return str ? `/albums?${str}` : '/albums';
  };

  return (
    <>
      <PageHeader title="앨범">
        <div className="page-meta">{count(albums.length)}장</div>
      </PageHeader>
      <div className="toolbar">
        <div className="seg" role="tablist" aria-label="정렬">
          {(Object.keys(SORTS) as SortKey[]).map((k) => (
            <Link key={k} href={href({ sort: k })} className={`seg-item${k === sort ? ' is-active' : ''}`} replace scroll={false}>
              {SORTS[k].label}
            </Link>
          ))}
        </div>
        <div className="chips" aria-label="음질">
          {TIERS.map((t) => (
            <Link
              key={t.key}
              href={href({ tier: t.key })}
              className={`chip${t.key === tier ? ' is-active' : ''}${t.key !== 'all' ? ` chip-${t.key}` : ''}`}
              replace
              scroll={false}
            >
              {t.label}
            </Link>
          ))}
        </div>
      </div>
      {albums.length ? (
        <AlbumGrid albums={albums.map(toCard)} showYear={sort === 'year'} />
      ) : (
        <p className="muted pad">이 조건에 맞는 앨범이 없습니다.</p>
      )}
    </>
  );
}
