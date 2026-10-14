import Link from 'next/link';
import { notFound } from 'next/navigation';
import type { Metadata } from 'next';
import { Backdrop } from '@/components/backdrop';
import { GeneratedArt } from '@/components/cover';
import { CircleAlert } from '@/components/icons';
import { PlayButtons } from '@/components/play-buttons';
import { QualityBadge } from '@/components/quality-badge';
import { Section } from '@/components/section';
import { TrackTable } from '@/components/track-table';
import { getShow, isValidId } from '@/lib/live/archive';
import { prettyDate } from '@/lib/live/shared';
import { fromLive } from '@/lib/player/tracks';
import { span } from '@/lib/time';

type Props = { params: Promise<{ id: string }> };

export async function generateMetadata({ params }: Props): Promise<Metadata> {
  const { id } = await params;
  const show = await getShow(decodeURIComponent(id)).catch(() => null);
  return { title: show ? `${show.band} — ${prettyDate(show.date)}` : '라이브 아카이브' };
}

export default async function LiveShowPage({ params }: Props) {
  const id = decodeURIComponent((await params).id);
  if (!isValidId(id)) notFound();

  let show: Awaited<ReturnType<typeof getShow>> = null;
  let error: string | null = null;
  try {
    show = await getShow(id);
  } catch (e) {
    error = e instanceof Error ? e.message : String(e);
  }
  if (error) {
    return (
      <div className="notice notice-error">
        <CircleAlert size={18} />
        <div>
          <strong>archive.org에 닿지 못했습니다.</strong>
          <div className="muted">{error}</div>
        </div>
      </div>
    );
  }
  if (!show) notFound();

  const tracks = show.tracks.map((t) => fromLive(show, t));
  const duration = tracks.reduce((s, t) => s + t.duration, 0);
  return (
    <div className="detail">
      <Backdrop seed={show.id} />
      <header className="detail-head">
        <div className="detail-art">
          <div className="cover">
            <GeneratedArt seed={show.id} title={show.band} subtitle={prettyDate(show.date)} size="lg" />
          </div>
        </div>
        <div className="detail-info">
          <div className="eyebrow">라이브 · Live Music Archive</div>
          <h1 className="detail-title">{show.band}</h1>
          <div className="detail-artist detail-artist-plain">
            {[prettyDate(show.date), show.venue, show.city].filter(Boolean).join(' · ')}
          </div>
          <div className="detail-meta">{[`${tracks.length}곡`, duration ? span(duration) : null].filter(Boolean).join(' · ')}</div>
          <div className="detail-tags">
            {tracks[0] && <QualityBadge format={tracks[0].format} />}
            <a className="tag" href={`https://archive.org/details/${encodeURIComponent(show.id)}`} target="_blank" rel="noreferrer">
              archive.org에서 보기
            </a>
          </div>
          <PlayButtons tracks={tracks} />
        </div>
      </header>

      {tracks.length ? (
        <TrackTable tracks={tracks} variant="live" />
      ) : (
        <p className="muted pad">이 공연에는 재생할 수 있는 파일이 없습니다.</p>
      )}

      {(show.notes || show.source || show.lineage || show.taper) && (
        <Section title="공연 정보">
          <div className="show-notes">
            {show.notes && <pre>{show.notes}</pre>}
            <dl>
              {show.taper && (
                <>
                  <dt>녹음</dt>
                  <dd>{show.taper}</dd>
                </>
              )}
              {show.source && (
                <>
                  <dt>소스</dt>
                  <dd>{show.source}</dd>
                </>
              )}
              {show.lineage && (
                <>
                  <dt>계보</dt>
                  <dd>{show.lineage}</dd>
                </>
              )}
            </dl>
          </div>
        </Section>
      )}
      <p className="credit">
        <Link href={`/live?q=${encodeURIComponent(show.band)}`}>{show.band}의 다른 공연</Link>
      </p>
    </div>
  );
}
