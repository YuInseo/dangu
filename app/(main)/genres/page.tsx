import Link from 'next/link';
import type { Metadata } from 'next';
import { Cover, GeneratedArt } from '@/components/cover';
import { EmptyLibrary } from '@/components/empty-library';
import { PageHeader } from '@/components/section';
import { config } from '@/lib/config';
import { getScanStatus, loadLibrary } from '@/lib/library/store';
import { coverUrls } from '@/lib/player/tracks';
import { count } from '@/lib/time';

export const metadata: Metadata = { title: '장르' };

export default async function GenresPage() {
  const lib = await loadLibrary();
  if (!lib.summary.tracks) return <EmptyLibrary status={getScanStatus()} musicDir={config.musicDir} />;
  return (
    <>
      <PageHeader title="장르">
        <div className="page-meta">{count(lib.genreList.length)}개</div>
      </PageHeader>
      {lib.genreList.length === 0 ? (
        <p className="muted pad">장르 태그가 있는 앨범이 없습니다.</p>
      ) : (
        <div className="grid grid-genres">
          {lib.genreList.map((g) => (
            <Link key={g.id} href={`/genres/${g.id}`} className="genre-tile" prefetch={false}>
              <div className="genre-mosaic">
                {g.covers.length >= 4 ? (
                  g.covers.slice(0, 4).map((c) => {
                    const u = coverUrls(c.albumId, c.cover)!;
                    return <Cover key={c.albumId} src={u.sm} color={u.color} alt="" seed={c.albumId} title="" size="sm" />;
                  })
                ) : g.covers[0] ? (
                  <Cover
                    src={coverUrls(g.covers[0].albumId, g.covers[0].cover)!.sm}
                    color={g.covers[0].cover.color}
                    alt=""
                    seed={g.id}
                    title=""
                    className="span-all"
                  />
                ) : (
                  <GeneratedArt seed={g.id} title="" size="sm" />
                )}
              </div>
              <div className="genre-name">{g.name}</div>
              <div className="genre-count">앨범 {count(g.albumIds.length)}장</div>
            </Link>
          ))}
        </div>
      )}
    </>
  );
}
