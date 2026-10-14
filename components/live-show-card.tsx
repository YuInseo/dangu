import Link from 'next/link';
import { prettyDate, LIVE_QUALITY_LABEL, type LiveShow } from '@/lib/live/shared';
import { GeneratedArt } from './cover';

export function LiveShowCard({ show }: { show: LiveShow }) {
  const q = LIVE_QUALITY_LABEL[show.quality];
  return (
    <Link href={`/live/${encodeURIComponent(show.id)}`} className="card" prefetch={false}>
      <div className="card-art">
        <div className="cover">
          <GeneratedArt seed={show.id} title={show.band} subtitle={prettyDate(show.date)} />
        </div>
        {q && <span className={`hires-mark live-q-${show.quality}`}>{q}</span>}
      </div>
      <div className="card-title" title={show.title}>
        {show.band}
      </div>
      <div className="card-sub">{[prettyDate(show.date), show.venue ?? show.city].filter(Boolean).join(' · ')}</div>
    </Link>
  );
}
