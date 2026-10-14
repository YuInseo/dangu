import { notFound } from 'next/navigation';
import type { Metadata } from 'next';
import { AlbumGrid } from '@/components/album-card';
import { PageHeader } from '@/components/section';
import { toCard } from '@/lib/library/build';
import { getLibrary, loadLibrary } from '@/lib/library/store';
import { count } from '@/lib/time';

type Props = { params: Promise<{ id: string }> };

export async function generateMetadata({ params }: Props): Promise<Metadata> {
  const { id } = await params;
  return { title: (await getLibrary()).genres.get(id)?.name ?? '장르' };
}

export default async function GenrePage({ params }: Props) {
  const { id } = await params;
  const lib = await loadLibrary();
  const genre = lib.genres.get(id);
  if (!genre) notFound();
  return (
    <>
      <PageHeader eyebrow="장르" title={genre.name}>
        <div className="page-meta">앨범 {count(genre.albumIds.length)}장</div>
      </PageHeader>
      <AlbumGrid albums={genre.albumIds.map((a) => toCard(lib.albums.get(a)!))} />
    </>
  );
}
