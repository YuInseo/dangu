import type { Metadata } from 'next';
import { ArtistGrid } from '@/components/artist-card';
import { EmptyLibrary } from '@/components/empty-library';
import { PageHeader } from '@/components/section';
import { config } from '@/lib/config';
import { getScanStatus, loadLibrary } from '@/lib/library/store';
import { count } from '@/lib/time';

export const metadata: Metadata = { title: '아티스트' };

export default async function ArtistsPage() {
  const lib = await loadLibrary();
  if (!lib.summary.tracks) return <EmptyLibrary status={getScanStatus()} musicDir={config.musicDir} />;
  return (
    <>
      <PageHeader title="아티스트">
        <div className="page-meta">{count(lib.artistList.length)}명</div>
      </PageHeader>
      <ArtistGrid artists={lib.artistList} />
    </>
  );
}
