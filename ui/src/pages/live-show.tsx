import { useMemo } from 'react';
import { useParams } from 'react-router';
import Link from '@/compat/link';
import { useApi, useTitle } from '@/api';
import { Backdrop } from '@/components/backdrop';
import { GeneratedArt } from '@/components/cover';
import { LiveSave } from '@/components/live-save';
import { PlayButtons } from '@/components/play-buttons';
import { QualityBadge } from '@/components/quality-badge';
import { Section } from '@/components/section';
import { TrackTable } from '@/components/track-table';
import { prettyDate, type LiveShowDetail } from '@/lib/live/shared';
import { fromLive } from '@/lib/player/tracks';
import { span } from '@/lib/time';
import { Failed, Loading } from './common';

export function LiveShowPage() {
  const { id = '' } = useParams();
  const { data: show, error } = useApi<LiveShowDetail>(`/api/live/show/${encodeURIComponent(id)}`);
  useTitle(show ? `${show.band} — ${prettyDate(show.date)}` : '라이브 아카이브');
  const tracks = useMemo(() => (show ? show.tracks.map((t) => fromLive(show, t)) : []), [show]);
  if (error) return <Failed error={error} />;
  if (!show) return <Loading />;
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
          <LiveSave show={show} />
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
