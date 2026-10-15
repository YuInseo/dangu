'use client';

import { useEffect, useState } from 'react';
import { libraryIds, openPlaylistPicker } from '@/lib/collections';
import { usePlayer, useQueue, useUi } from '@/lib/player/context';
import type { QueueItem } from '@/lib/player/types';
import { clock, span } from '@/lib/time';
import { Cover } from './cover';
import { GripVertical, Save, Trash2, X } from './icons';
import { Equalizer } from './track-table';
import { useEngineState } from '@/lib/player/context';

function QueueRow({
  item,
  index,
  current,
  dim,
  dragging,
  over,
  onDragStart,
  onDragOver,
  onDrop,
}: {
  item: QueueItem;
  index: number;
  current?: boolean;
  dim?: boolean;
  dragging?: boolean;
  over?: boolean;
  onDragStart?: (i: number) => void;
  onDragOver?: (i: number) => void;
  onDrop?: (i: number) => void;
}) {
  const { jump, remove } = usePlayer();
  const { status } = useEngineState();
  const t = item.track;
  return (
    <div
      className={`qrow${current ? ' is-current' : ''}${dim ? ' is-dim' : ''}${dragging ? ' is-dragging' : ''}${over ? ' is-over' : ''}`}
      draggable={!current && !!onDragStart}
      onDragStart={(e) => {
        e.dataTransfer.effectAllowed = 'move';
        e.dataTransfer.setData('text/plain', String(index));
        onDragStart?.(index);
      }}
      onDragOver={(e) => {
        if (!onDragOver) return;
        e.preventDefault();
        onDragOver(index);
      }}
      onDrop={(e) => {
        e.preventDefault();
        onDrop?.(index);
      }}
      onDragEnd={() => onDrop?.(-1)}
      onDoubleClick={() => !current && jump(index)}
    >
      {!current && onDragStart && (
        <span className="qrow-grip" aria-hidden>
          <GripVertical size={14} />
        </span>
      )}
      <button type="button" className="qrow-main" onClick={() => !current && jump(index)} disabled={current}>
        <span className="qrow-art">
          <Cover src={t.cover?.sm} color={t.cover?.color} alt="" seed={t.art.seed} title={t.art.title} size="sm" />
          {current && (
            <span className="qrow-eq">
              <Equalizer paused={status !== 'playing' && status !== 'loading'} />
            </span>
          )}
        </span>
        <span className="qrow-text">
          <span className="qrow-title">{t.title}</span>
          <span className="qrow-sub">{t.artist}</span>
        </span>
        <span className="qrow-time">{clock(t.duration)}</span>
      </button>
      {!current && (
        <button type="button" className="icon-btn qrow-remove" onClick={() => remove(index)} aria-label="대기열에서 빼기">
          <X size={15} />
        </button>
      )}
    </div>
  );
}

export function QueuePanel() {
  const { queue: open, setQueue } = useUi();
  const { items, index, current } = useQueue();
  const { move, clearUpcoming, clearAll } = usePlayer();
  const [showHistory, setShowHistory] = useState(false);
  const [drag, setDrag] = useState<number | null>(null);
  const [over, setOver] = useState<number | null>(null);

  useEffect(() => {
    if (!open) return;
    const key = (e: KeyboardEvent) => e.key === 'Escape' && setQueue(false);
    document.addEventListener('keydown', key);
    return () => document.removeEventListener('keydown', key);
  }, [open, setQueue]);

  const upcoming = items.slice(index + 1);
  const saveable = libraryIds(items.map((it) => it.track));
  const remaining = upcoming.reduce((s, i) => s + i.track.duration, 0);

  const drop = (to: number) => {
    if (drag != null && to >= 0 && to !== drag) move(drag, to);
    setDrag(null);
    setOver(null);
  };

  return (
    <aside className={`queue${open ? ' is-open' : ''}`} aria-label="대기열" aria-hidden={!open} inert={!open}>
      <header className="queue-head">
        <h2>대기열</h2>
        <div className="queue-head-actions">
          {upcoming.length > 0 && (
            <button type="button" className="text-btn" onClick={clearUpcoming}>
              다음 곡 비우기
            </button>
          )}
          {saveable.length > 0 && (
            <button
              type="button"
              className="icon-btn"
              onClick={() =>
                openPlaylistPicker({
                  ids: saveable,
                  label: `대기열 ${saveable.length}곡`,
                  suggest: `대기열 ${new Date().toLocaleDateString('ko-KR', { month: 'long', day: 'numeric' })}`,
                })
              }
              aria-label="플레이리스트로 저장"
              title="플레이리스트로 저장"
            >
              <Save size={17} />
            </button>
          )}
          {items.length > 0 && (
            <button type="button" className="icon-btn" onClick={clearAll} aria-label="대기열 전부 비우기" title="전부 비우기">
              <Trash2 size={17} />
            </button>
          )}
          <button type="button" className="icon-btn" onClick={() => setQueue(false)} aria-label="닫기">
            <X size={19} />
          </button>
        </div>
      </header>

      <div className="queue-body">
        {!current && <p className="queue-empty">대기열이 비어 있습니다. 앨범이나 곡을 재생하면 여기에 쌓입니다.</p>}

        {index > 0 && (
          <>
            <button type="button" className="queue-history-toggle" onClick={() => setShowHistory((s) => !s)}>
              {showHistory ? '지난 곡 숨기기' : `지난 곡 ${index}개 보기`}
            </button>
            {showHistory && items.slice(0, index).map((it, i) => <QueueRow key={it.uid} item={it} index={i} dim />)}
          </>
        )}

        {current && (
          <>
            <div className="queue-label">지금 재생 중</div>
            <QueueRow item={current} index={index} current />
          </>
        )}

        {upcoming.length > 0 && (
          <>
            <div className="queue-label">
              다음 곡{' '}
              <span className="muted">
                · {upcoming.length}곡 · {span(remaining)}
              </span>
            </div>
            {upcoming.map((it, k) => {
              const i = index + 1 + k;
              return (
                <QueueRow
                  key={it.uid}
                  item={it}
                  index={i}
                  dragging={drag === i}
                  over={over === i && drag !== i}
                  onDragStart={setDrag}
                  onDragOver={setOver}
                  onDrop={drop}
                />
              );
            })}
          </>
        )}
      </div>
    </aside>
  );
}
