import { useParams } from 'react-router';
import { useApi, useTitle } from '@/api';
import { AlbumGrid } from '@/components/album-card';
import { PageHeader } from '@/components/section';
import type { AlbumCard, Genre } from '@/lib/library/types';
import { count } from '@/lib/time';
import { Failed, Loading } from './common';

export function GenrePage() {
  const { id = '' } = useParams();
  const { data, error } = useApi<{ genre: Genre; albums: AlbumCard[] }>(`/api/genres/${encodeURIComponent(id)}`);
  useTitle(data?.genre.name ?? '장르');
  if (error) return <Failed error={error} />;
  if (!data) return <Loading />;
  return (
    <>
      <PageHeader eyebrow="장르" title={data.genre.name}>
        <div className="page-meta">앨범 {count(data.genre.albumIds.length)}장</div>
      </PageHeader>
      <AlbumGrid albums={data.albums} />
    </>
  );
}
