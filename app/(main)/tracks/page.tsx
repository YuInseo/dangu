import type { Metadata } from 'next';
import { EmptyLibrary } from '@/components/empty-library';
import { PageHeader } from '@/components/section';
import { TracksBrowser } from '@/components/tracks-browser';
import { config } from '@/lib/config';
import { getScanStatus, loadLibrary } from '@/lib/library/store';
import { count } from '@/lib/time';

export const metadata: Metadata = { title: '곡' };

export default async function TracksPage() {
  const lib = await loadLibrary();
  if (!lib.summary.tracks) return <EmptyLibrary status={getScanStatus()} musicDir={config.musicDir} />;
  return (
    <>
      <PageHeader title="곡">
        <div className="page-meta">{count(lib.summary.tracks)}곡</div>
      </PageHeader>
      <TracksBrowser initial={lib.trackList.slice(0, 200)} total={lib.trackList.length} />
    </>
  );
}
