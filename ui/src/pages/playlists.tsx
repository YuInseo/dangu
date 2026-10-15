import { useState } from 'react';
import Link from '@/compat/link';
import { useRouter } from '@/compat/navigation';
import { fetchJson, useApi, useTitle } from '@/api';
import { LoaderCircle, Play, Plus } from '@/components/icons';
import { PlaylistArt } from '@/components/playlist-art';
import { PageHeader } from '@/components/section';
import { useToast } from '@/components/toast';
import { playlists, type PlaylistDetail, type PlaylistSummary } from '@/lib/collections';
import { usePlayer } from '@/lib/player/context';
import { fromLibrary } from '@/lib/player/tracks';
import { count, span } from '@/lib/time';
import { Failed, Loading } from './common';
import { CollectionEmpty } from './favorites';

export function PlaylistsPage() {
  useTitle('플레이리스트');
  const { data, error } = useApi<PlaylistSummary[]>('/api/playlists');
  if (error) return <Failed error={error} />;
  if (!data) return <Loading />;
  return (
    <>
      <PageHeader eyebrow="보관함" title="플레이리스트">
        <div className="page-meta">{data.length ? `${count(data.length)}개` : ''}</div>
      </PageHeader>
      <div className="toolbar">
        <NewPlaylist />
      </div>
      {data.length ? (
        <div className="grid">
          {data.map((p) => (
            <PlaylistCard key={p.id} playlist={p} />
          ))}
        </div>
      ) : (
        <CollectionEmpty title="아직 플레이리스트가 없습니다">
          위의 ‘새 플레이리스트’로 만들거나, 곡의 ⋯ 메뉴와 대기열에서 바로 담을 수 있습니다.
        </CollectionEmpty>
      )}
    </>
  );
}

/** 이름을 받아 빈 플레이리스트를 만들고 그리로 간다. */
export function NewPlaylist({ compact }: { compact?: boolean }) {
  const [open, setOpen] = useState(false);
  const [name, setName] = useState('');
  const [busy, setBusy] = useState(false);
  const router = useRouter();
  const toast = useToast();

  if (!open) {
    return (
      <button type="button" className={`btn${compact ? ' btn-sm' : ''}`} onClick={() => setOpen(true)}>
        <Plus size={compact ? 16 : 18} /> 새 플레이리스트
      </button>
    );
  }
  return (
    <form
      className="pl-new pl-new-inline"
      onSubmit={async (e) => {
        e.preventDefault();
        if (!name.trim() || busy) return;
        setBusy(true);
        try {
          const p = await playlists.create(name);
          router.push(`/playlists/${p.id}`);
        } catch (err) {
          toast(err instanceof Error && err.message ? err.message : '만들지 못했습니다', 'error');
          setBusy(false);
        }
      }}
    >
      <input
        autoFocus
        className="pl-new-input"
        value={name}
        maxLength={100}
        placeholder="새 플레이리스트 이름"
        aria-label="새 플레이리스트 이름"
        onChange={(e) => setName(e.target.value)}
        onKeyDown={(e) => e.key === 'Escape' && setOpen(false)}
      />
      <button type="submit" className="btn btn-primary btn-sm" disabled={!name.trim() || busy}>
        만들기
      </button>
      <button type="button" className="text-btn" onClick={() => setOpen(false)}>
        취소
      </button>
    </form>
  );
}

function PlaylistCard({ playlist: p }: { playlist: PlaylistSummary }) {
  const { playTracks } = usePlayer();
  const toast = useToast();
  const [busy, setBusy] = useState(false);

  async function play(e: React.MouseEvent) {
    e.preventDefault();
    e.stopPropagation();
    if (busy) return;
    setBusy(true);
    try {
      const d = await fetchJson<PlaylistDetail>(`/api/playlists/${p.id}`);
      if (d.tracks.length) playTracks(d.tracks.map(fromLibrary));
    } catch {
      toast('플레이리스트를 불러오지 못했습니다', 'error');
    } finally {
      setBusy(false);
    }
  }

  return (
    <Link href={`/playlists/${p.id}`} className="card" prefetch={false}>
      <div className="card-art">
        <PlaylistArt playlist={p} />
        {p.count > 0 && (
          <button type="button" className="card-play" onClick={play} aria-label={`${p.name} 재생`}>
            {busy ? <LoaderCircle className="spin" size={20} /> : <Play size={20} fill="currentColor" />}
          </button>
        )}
      </div>
      <div className="card-title" title={p.name}>
        {p.name}
      </div>
      <div className="card-sub">
        {count(p.count)}곡{p.duration ? ` · ${span(p.duration)}` : ''}
      </div>
    </Link>
  );
}
