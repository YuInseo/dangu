import { useApi, useTitle } from '@/api';
import { EmptyLibrary } from '@/components/empty-library';
import { PageHeader } from '@/components/section';
import { TracksBrowser } from '@/components/tracks-browser';
import type { Track } from '@/lib/library/types';
import { count } from '@/lib/time';
import { useLibraryInfo } from '@/library';
import { Failed, Loading } from './common';

export function TracksPage() {
  useTitle('곡');
  const info = useLibraryInfo();
  const { data, error } = useApi<{ total: number; tracks: Track[] }>('/api/tracks?offset=0&limit=200');
  if (!info.summary.tracks) return <EmptyLibrary status={info.status} musicDir={info.musicDir} />;
  if (error) return <Failed error={error} />;
  if (!data) return <Loading />;
  return (
    <>
      <PageHeader title="곡">
        <div className="page-meta">{count(info.summary.tracks)}곡</div>
      </PageHeader>
      <TracksBrowser key={info.generation} initial={data.tracks} total={data.total} />
    </>
  );
}
