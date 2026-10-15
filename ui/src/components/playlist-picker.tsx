'use client';

import { useEffect, useRef, useState } from 'react';
import { createPortal } from 'react-dom';
import { useApi } from '@/api';
import { closePlaylistPicker, playlists, usePlaylistPicker, type PickerRequest, type PlaylistSummary } from '@/lib/collections';
import { count } from '@/lib/time';
import { Plus, X } from './icons';
import { PlaylistArt } from './playlist-art';
import { useToast } from './toast';

/** "플레이리스트에 추가" 창. 셸에 하나 두고, 어디서든 `openPlaylistPicker`로 연다. */
export function PlaylistPicker() {
  const req = usePlaylistPicker();
  if (!req) return null;
  return <PickerDialog req={req} />;
}

function PickerDialog({ req }: { req: PickerRequest }) {
  const { data } = useApi<PlaylistSummary[]>('/api/playlists');
  const toast = useToast();
  const [creating, setCreating] = useState(false);
  const [name, setName] = useState(req.suggest ?? '');
  const [busy, setBusy] = useState(false);
  const box = useRef<HTMLDivElement>(null);
  const n = req.ids.length;
  const form = creating || data?.length === 0;

  useEffect(() => {
    const key = (e: KeyboardEvent) => e.key === 'Escape' && closePlaylistPicker();
    document.addEventListener('keydown', key);
    document.documentElement.classList.add('no-scroll');
    return () => {
      document.removeEventListener('keydown', key);
      document.documentElement.classList.remove('no-scroll');
    };
  }, []);

  useEffect(() => {
    if (!form) box.current?.querySelector<HTMLElement>('.pl-pick')?.focus();
  }, [form]);

  const addTo = async (p: PlaylistSummary) => {
    setBusy(true);
    try {
      await playlists.add(p.id, req.ids);
      toast(`${p.name}에 ${count(n)}곡을 넣었습니다`);
      closePlaylistPicker();
    } catch {
      toast('플레이리스트에 넣지 못했습니다', 'error');
      setBusy(false);
    }
  };

  const create = async () => {
    if (!name.trim() || busy) return;
    setBusy(true);
    try {
      const p = await playlists.create(name, req.ids);
      toast(`새 플레이리스트 ${p.name} · ${count(n)}곡`);
      closePlaylistPicker();
    } catch (e) {
      toast(e instanceof Error && e.message ? e.message : '만들지 못했습니다', 'error');
      setBusy(false);
    }
  };

  return createPortal(
    <div className="sheet-backdrop" onPointerDown={(e) => e.target === e.currentTarget && closePlaylistPicker()}>
      <div ref={box} className="sheet" role="dialog" aria-modal="true" aria-labelledby="pl-picker-title">
        <header className="sheet-head">
          <div className="sheet-head-text">
            <div className="sheet-title" id="pl-picker-title">
              플레이리스트에 추가
            </div>
            <div className="sheet-sub">{req.label}</div>
          </div>
          <button type="button" className="icon-btn" onClick={closePlaylistPicker} aria-label="닫기">
            <X size={19} />
          </button>
        </header>

        <div className="sheet-body">
          {form ? (
            <form
              className="pl-new"
              onSubmit={(e) => {
                e.preventDefault();
                void create();
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
              />
              <button type="submit" className="btn btn-primary btn-sm" disabled={!name.trim() || busy}>
                만들기
              </button>
            </form>
          ) : (
            <button type="button" className="pl-pick" onClick={() => setCreating(true)}>
              <span className="pl-pick-art pl-pick-plus">
                <Plus size={22} />
              </span>
              <span className="pl-pick-text">
                <span className="pl-pick-name">새 플레이리스트</span>
              </span>
            </button>
          )}

          {data?.map((p) => (
            <button key={p.id} type="button" className="pl-pick" onClick={() => void addTo(p)} disabled={busy}>
              <span className="pl-pick-art">
                <PlaylistArt playlist={p} size="sm" />
              </span>
              <span className="pl-pick-text">
                <span className="pl-pick-name">{p.name}</span>
                <span className="pl-pick-sub">{count(p.count)}곡</span>
              </span>
            </button>
          ))}
        </div>
      </div>
    </div>,
    document.body,
  );
}
