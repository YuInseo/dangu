import Link from '@/compat/link';
import { useSearchParams } from '@/compat/navigation';
import { useApi, useTitle } from '@/api';
import { AlbumGrid } from '@/components/album-card';
import { EmptyLibrary } from '@/components/empty-library';
import { PageHeader } from '@/components/section';
import type { AlbumCard, QualityTier } from '@/lib/library/types';
import { count } from '@/lib/time';
import { useLibraryInfo } from '@/library';
import { Failed, Loading } from './common';

const SORTS = { artist: '아티스트', title: '제목', year: '연도', added: '최근 추가' } as const;
type SortKey = keyof typeof SORTS;

const TIERS: { key: QualityTier | 'all'; label: string }[] = [
  { key: 'all', label: '전체' },
  { key: 'hires', label: 'Hi-Res' },
  { key: 'lossless', label: '무손실' },
  { key: 'lossy', label: '손실 압축' },
];

export function AlbumsPage() {
  useTitle('앨범');
  const info = useLibraryInfo();
  const sp = useSearchParams();
  const sortParam = sp.get('sort') ?? '';
  const sort: SortKey = sortParam in SORTS ? (sortParam as SortKey) : 'artist';
  const tierParam = sp.get('tier');
  const tier = TIERS.some((t) => t.key === tierParam) ? (tierParam as QualityTier | 'all') : 'all';
  const { data, error } = useApi<{ albums: AlbumCard[] }>(`/api/albums?sort=${sort}&tier=${tier}`);

  if (!info.summary.tracks) return <EmptyLibrary status={info.status} musicDir={info.musicDir} />;

  const href = (p: { sort?: string; tier?: string }) => {
    const q = new URLSearchParams();
    const s = p.sort ?? sort;
    const t = p.tier ?? tier;
    if (s !== 'artist') q.set('sort', s);
    if (t !== 'all') q.set('tier', t);
    const str = q.toString();
    return str ? `/albums?${str}` : '/albums';
  };
  const albums = data?.albums;

  return (
    <>
      <PageHeader title="앨범">
        <div className="page-meta">{albums ? `${count(albums.length)}장` : ''}</div>
      </PageHeader>
      <div className="toolbar">
        <div className="seg" role="tablist" aria-label="정렬">
          {(Object.keys(SORTS) as SortKey[]).map((k) => (
            <Link key={k} href={href({ sort: k })} className={`seg-item${k === sort ? ' is-active' : ''}`} replace scroll={false}>
              {SORTS[k]}
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
      {error ? (
        <Failed error={error} />
      ) : !albums ? (
        <Loading />
      ) : albums.length ? (
        <AlbumGrid albums={albums} showYear={sort === 'year'} />
      ) : (
        <p className="muted pad">이 조건에 맞는 앨범이 없습니다.</p>
      )}
    </>
  );
}
