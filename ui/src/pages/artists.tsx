import { useApi, useTitle } from '@/api';
import { ArtistGrid } from '@/components/artist-card';
import { EmptyLibrary } from '@/components/empty-library';
import { PageHeader } from '@/components/section';
import type { Artist } from '@/lib/library/types';
import { count } from '@/lib/time';
import { useLibraryInfo } from '@/library';
import { Failed, Loading } from './common';

export function ArtistsPage() {
  useTitle('아티스트');
  const info = useLibraryInfo();
  const { data, error } = useApi<{ artists: Artist[] }>('/api/artists');
  if (!info.summary.tracks) return <EmptyLibrary status={info.status} musicDir={info.musicDir} />;
  if (error) return <Failed error={error} />;
  if (!data) return <Loading />;
  return (
    <>
      <PageHeader title="아티스트">
        <div className="page-meta">{count(data.artists.length)}명</div>
      </PageHeader>
      <ArtistGrid artists={data.artists} />
    </>
  );
}
