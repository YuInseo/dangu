import { useMemo, useState } from 'react';
import { useParams } from 'react-router';
import { useRouter } from '@/compat/navigation';
import { useApi, useTitle } from '@/api';
import { Backdrop } from '@/components/backdrop';
import { Cover } from '@/components/cover';
import { ArrowDown, ArrowUp, GripVertical, Pencil, Trash2, X } from '@/components/icons';
import { PlayButtons } from '@/components/play-buttons';
import { PlaylistArt } from '@/components/playlist-art';
import { TrackTable } from '@/components/track-table';
import { useToast } from '@/components/toast';
import { moveItem, playlists, type PlaylistDetail } from '@/lib/collections';
import type { Track } from '@/lib/library/types';
import { coverUrls, fromLibrary } from '@/lib/player/tracks';
import { ago, clock, count, span } from '@/lib/time';
import { Failed, Loading } from './common';
import { CollectionEmpty } from './favorites';

export function PlaylistPage() {
  const { id = '' } = useParams();
  const { data, error } = useApi<PlaylistDetail>(`/api/playlists/${encodeURIComponent(id)}`);
  const [editing, setEditing] = useState(false);
  useTitle(data?.name ?? '플레이리스트');
  const tracks = useMemo(() => data?.tracks.map(fromLibrary) ?? [], [data]);
  if (error) return <Failed error={error} />;
  if (!data) return <Loading />;
  if (editing) return <PlaylistEditor playlist={data} onDone={() => setEditing(false)} />;
  const first = data.covers[0];
  const bg = first ? coverUrls(first.albumId, first.cover) : undefined;

  return (
    <div className="detail">
      <Backdrop src={bg?.sm} color={bg?.color} />
      <header className="detail-head">
        <div className="detail-art">
          <PlaylistArt playlist={data} size="lg" />
        </div>
        <div className="detail-info">
          <div className="eyebrow">플레이리스트</div>
          <h1 className="detail-title">{data.name}</h1>
          <div className="detail-meta">
            {[`${count(data.count)}곡`, data.duration ? span(data.duration) : null, `${ago(data.updated)} 고침`].filter(Boolean).join(' · ')}
          </div>
          {data.missing > 0 && <div className="detail-note">{count(data.missing)}곡은 지금 라이브러리에서 찾을 수 없습니다</div>}
          <div className="detail-gap" />
          <PlayButtons tracks={tracks} label={data.name}>
            <button type="button" className="icon-btn icon-btn-lg" onClick={() => setEditing(true)} aria-label="편집" title="이름 바꾸기·순서·빼기·지우기">
              <Pencil size={18} />
            </button>
          </PlayButtons>
        </div>
      </header>

      {tracks.length ? (
        <TrackTable tracks={tracks} variant="list" />
      ) : (
        <CollectionEmpty title="빈 플레이리스트입니다">
          곡이나 앨범의 ⋯ 메뉴에서 ‘플레이리스트에 추가’를 누르거나, 대기열을 통째로 담을 수 있습니다.
        </CollectionEmpty>
      )}
    </div>
  );
}

interface Row {
  /** 같은 곡이 두 번 있어도 칸마다 다른 열쇠 */
  k: number;
  id: string;
}

/** 편집: 이름, 순서(끌거나 위·아래 단추), 빼기, 통째로 지우기. ‘완료’를 누를 때 한 번에 저장한다. */
function PlaylistEditor({ playlist, onDone }: { playlist: PlaylistDetail; onDone: () => void }) {
  const byId = useMemo(() => new Map<string, Track>(playlist.tracks.map((t) => [t.id, t])), [playlist]);
  const [name, setName] = useState(playlist.name);
  const [rows, setRows] = useState<Row[]>(() => playlist.ids.map((id, k) => ({ k, id })));
  const [drag, setDrag] = useState<number | null>(null);
  const [over, setOver] = useState<number | null>(null);
  const [busy, setBusy] = useState(false);
  const [armed, setArmed] = useState(false);
  const router = useRouter();
  const toast = useToast();
  const ids = rows.map((r) => r.id);
  const sameTracks = ids.length === playlist.ids.length && ids.every((id, i) => id === playlist.ids[i]);
  const sameName = name.trim() === playlist.name;

  const save = async () => {
    if (!name.trim()) {
      toast('이름을 적어 주세요', 'error');
      return;
    }
    if (sameName && sameTracks) {
      onDone();
      return;
    }
    setBusy(true);
    try {
      await playlists.update(playlist.id, { name: sameName ? undefined : name, tracks: sameTracks ? undefined : ids });
      onDone();
    } catch (e) {
      toast(e instanceof Error && e.message ? e.message : '저장하지 못했습니다', 'error');
      setBusy(false);
    }
  };

  const remove = async () => {
    if (!armed) {
      setArmed(true);
      setTimeout(() => setArmed(false), 4000);
      return;
    }
    setBusy(true);
    try {
      await playlists.remove(playlist.id);
      toast(`${playlist.name} 플레이리스트를 지웠습니다`);
      router.replace('/playlists');
    } catch {
      toast('지우지 못했습니다', 'error');
      setBusy(false);
    }
  };

  const drop = (to: number) => {
    if (drag != null && to >= 0 && to !== drag) setRows(moveItem(rows, drag, to));
    setDrag(null);
    setOver(null);
  };

  return (
    <div className="pl-edit">
      <header className="pl-edit-head">
        <div className="eyebrow">플레이리스트 편집</div>
        <input
          className="pl-edit-name"
          value={name}
          maxLength={100}
          aria-label="플레이리스트 이름"
          onChange={(e) => setName(e.target.value)}
          onKeyDown={(e) => e.key === 'Enter' && void save()}
        />
        <div className="actions">
          <button type="button" className="btn btn-primary" onClick={() => void save()} disabled={busy}>
            완료
          </button>
          <button type="button" className="btn" onClick={onDone} disabled={busy}>
            취소
          </button>
          <button type="button" className={`btn btn-danger${armed ? ' is-armed' : ''}`} onClick={() => void remove()} disabled={busy}>
            <Trash2 size={16} /> {armed ? '한 번 더 누르면 지웁니다' : '플레이리스트 지우기'}
          </button>
        </div>
      </header>

      {rows.length === 0 ? (
        <CollectionEmpty title="곡이 없습니다" />
      ) : (
        <div className="pl-edit-list" role="list" aria-label="곡 순서">
          {rows.map((r, i) => {
            const t = byId.get(r.id);
            const cover = t ? coverUrls(t.albumId, t.cover) : undefined;
            return (
              <div
                key={r.k}
                role="listitem"
                className={`pl-erow${drag === i ? ' is-dragging' : ''}${over === i && drag !== i ? ' is-over' : ''}${t ? '' : ' is-missing'}`}
                draggable
                onDragStart={(e) => {
                  e.dataTransfer.effectAllowed = 'move';
                  e.dataTransfer.setData('text/plain', String(i));
                  setDrag(i);
                }}
                onDragOver={(e) => {
                  e.preventDefault();
                  setOver(i);
                }}
                onDrop={(e) => {
                  e.preventDefault();
                  drop(i);
                }}
                onDragEnd={() => drop(-1)}
              >
                <span className="pl-erow-grip" aria-hidden>
                  <GripVertical size={15} />
                </span>
                <span className="pl-erow-art">
                  {t ? (
                    <Cover src={cover?.sm} color={cover?.color} alt="" seed={t.albumId} title={t.album} size="sm" />
                  ) : (
                    <span className="pl-erow-gone" />
                  )}
                </span>
                <span className="pl-erow-text">
                  <span className="pl-erow-title">{t ? t.title : '라이브러리에서 찾을 수 없는 곡'}</span>
                  <span className="pl-erow-sub">{t ? `${t.artist} · ${clock(t.duration)}` : '파일을 옮겼거나 지웠습니다'}</span>
                </span>
                <span className="pl-erow-actions">
                  <button type="button" className="icon-btn" disabled={i === 0} onClick={() => setRows(moveItem(rows, i, i - 1))} aria-label="위로">
                    <ArrowUp size={17} />
                  </button>
                  <button
                    type="button"
                    className="icon-btn"
                    disabled={i === rows.length - 1}
                    onClick={() => setRows(moveItem(rows, i, i + 1))}
                    aria-label="아래로"
                  >
                    <ArrowDown size={17} />
                  </button>
                  <button type="button" className="icon-btn" onClick={() => setRows(rows.filter((_, k) => k !== i))} aria-label="빼기">
                    <X size={17} />
                  </button>
                </span>
              </div>
            );
          })}
        </div>
      )}
    </div>
  );
}
