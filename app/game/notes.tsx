'use client';

import { useCallback, useEffect, useRef, useState } from 'react';

import { inkOf, type Ink, type NotePage, type Stroke } from '../../lib/game';

/**
 * 이 판의 노트.
 *
 * 당구장에서 종이에 적는 것들이 있다. 오늘의 내기 조건, 셋이 돌아가며 칠 때의 순서,
 * 방금 놓친 배치. 앞의 둘은 글이고 마지막은 그림인데, 대개 한 장에 같이 적힌다 —
 * 순서를 적고 그 옆에 자리를 그린다. 그래서 한 장이 글과 그림을 함께 갖는다.
 *
 * 도구는 아래 한 줄에 있다. 글·펜·형광펜·지우개, 그리고 되돌리기. 손이 화면 아래쪽에
 * 있는 폰에서 도구를 위에 두면 그릴 때마다 손이 왕복한다.
 */
export function Notes({
  pages,
  onChange,
  onClose,
}: {
  pages: NotePage[];
  onChange: (pages: NotePage[]) => void;
  onClose: () => void;
}) {
  return (
    <div
      className="sheet"
      role="dialog"
      aria-modal="true"
      aria-label="노트"
      onClick={(event) => {
        if (event.target === event.currentTarget) onClose();
      }}
    >
      <div className="inner notes" style={{ textAlign: 'left' }}>
        <div className="sheet-head">
          <strong>노트</strong>
          <button className="ghost" onClick={onClose} aria-label="닫기">
            ✕
          </button>
        </div>

        <NotePages pages={pages} onChange={onChange} />
      </div>
    </div>
  );
}

/* 장 ----------------------------------------------------------------- */

const PEN_COLORS = ['#f3f4f6', '#ef4444', '#f59e0b', '#22c55e', '#3b82f6', '#a855f7'];
const HIGHLIGHT_COLORS = ['#facc15', '#4ade80', '#38bdf8', '#fb7185'];

type Tool = 'text' | 'pen' | 'highlighter' | 'eraser';

/**
 * 장 목록과 지금 보고 있는 장.
 *
 * 시트에서 떼어 둔 이유는 기록 화면에도 같은 것이 필요하기 때문이다 — 치는 중에 적는
 * 자리와 나중에 다시 보는 자리가 다른 모양이면, 같은 노트를 두 번 배워야 한다.
 */
export function NotePages({
  pages,
  onChange,
}: {
  pages: NotePage[];
  onChange: (pages: NotePage[]) => void;
}) {
  const [at, setAt] = useState(0);
  const [tool, setTool] = useState<Tool>('pen');
  const [width, setWidth] = useState(4);
  const [penColor, setPenColor] = useState(PEN_COLORS[0]);
  const [markColor, setMarkColor] = useState(HIGHLIGHT_COLORS[0]);
  /** 되돌린 획들. 저장하지 않는다 — 노트를 다시 열 때까지 남을 성질의 것이 아니다. */
  const [undone, setUndone] = useState<Stroke[]>([]);

  const index = Math.min(at, Math.max(0, pages.length - 1));
  const current = pages[index];
  const strokes = current?.strokes ?? [];

  const replace = (page: NotePage) =>
    onChange(pages.map((entry, position) => (position === index ? page : entry)));

  const setStrokes = (next: Stroke[]) => {
    if (!current) return;
    replace({ ...current, strokes: next });
  };

  const add = () => {
    const id = `n-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 6)}`;
    onChange([...pages, { id }]);
    setAt(pages.length);
    setUndone([]);
  };

  const remove = () => {
    const next = pages.filter((_, position) => position !== index);
    onChange(next);
    setAt(Math.max(0, Math.min(index, next.length - 1)));
    setUndone([]);
  };

  const color = tool === 'highlighter' ? markColor : penColor;

  return (
    <div className="notes-body">
      {/* 장이 여럿이면 위에 번호로 선다. 한 장뿐이어도 "+"는 늘 보인다. */}
      <div className="tabs" role="tablist">
        {pages.map((page, position) => (
          <button
            key={page.id}
            role="tab"
            aria-selected={position === index}
            className="tab"
            onClick={() => {
              setAt(position);
              setUndone([]);
            }}
          >
            {position + 1}
          </button>
        ))}
        <button className="tab add" onClick={add} aria-label="장 추가">
          + 장
        </button>
        {pages.length > 0 && (
          <button className="tab add" onClick={remove} aria-label="이 장 지우기">
            장 지우기
          </button>
        )}
      </div>

      {pages.length === 0 && (
        <p className="notice">
          <strong>+ 장</strong>을 눌러 시작하세요. 여기 적은 것은 이 판의 기록에 함께 남습니다.
        </p>
      )}

      {current && (
        <>
          {/*
            글은 그림 위가 아니라 위쪽 칸에 따로 있다.

            진짜 노트 앱은 손글씨를 글자 레이어 위에 겹쳐 놓지만, 그건 글자가 흐를 때마다
            획이 따라 움직여야 한다는 뜻이다. 여기 적는 글은 대개 두세 줄이라 그 복잡함이
            값을 하지 않는다 — 위에 글, 아래에 그림이면 둘 다 온전히 쓸 수 있다.
          */}
          {(tool === 'text' || (current.text ?? '').length > 0) && (
            <textarea
              className="note-text"
              value={current.text ?? ''}
              placeholder="내기 조건, 순서, 무엇이든"
              onChange={(event) => replace({ ...current, text: event.target.value })}
            />
          )}

          <Sketch
            strokes={strokes}
            tool={tool}
            color={color}
            width={width}
            onDraw={(stroke) => {
              setStrokes([...strokes, stroke]);
              setUndone([]);
            }}
            onErase={(keep) => {
              setStrokes(keep);
              setUndone([]);
            }}
          />

          {/*
            도구 줄.

            고르면 그 도구의 설정이 바로 아래 펼쳐진다. 설정을 따로 띄우는 창에 넣으면
            굵기 한 번 바꾸는 데 열고·고르고·닫는 세 번이 든다.
          */}
          <div className="tools">
            <button className="tool" aria-pressed={tool === 'text'} onClick={() => setTool('text')}>
              글
            </button>
            <button className="tool" aria-pressed={tool === 'pen'} onClick={() => setTool('pen')}>
              펜
            </button>
            <button
              className="tool"
              aria-pressed={tool === 'highlighter'}
              onClick={() => setTool('highlighter')}
            >
              형광펜
            </button>
            <button
              className="tool"
              aria-pressed={tool === 'eraser'}
              onClick={() => setTool('eraser')}
            >
              지우개
            </button>

            <span className="gap" />

            <button
              className="tool"
              disabled={strokes.length === 0}
              aria-label="되돌리기"
              onClick={() => {
                const last = strokes[strokes.length - 1];
                setStrokes(strokes.slice(0, -1));
                setUndone([...undone, last]);
              }}
            >
              ↶
            </button>
            <button
              className="tool"
              disabled={undone.length === 0}
              aria-label="다시 하기"
              onClick={() => {
                const back = undone[undone.length - 1];
                setUndone(undone.slice(0, -1));
                setStrokes([...strokes, back]);
              }}
            >
              ↷
            </button>
          </div>

          {(tool === 'pen' || tool === 'highlighter') && (
            <div className="ink">
              <input
                type="range"
                min={1}
                max={10}
                value={width}
                aria-label="두께"
                onChange={(event) => setWidth(Number(event.target.value))}
              />
              <div className="swatches">
                {(tool === 'pen' ? PEN_COLORS : HIGHLIGHT_COLORS).map((value) => (
                  <button
                    key={value}
                    className="swatch"
                    aria-label={`색 ${value}`}
                    aria-pressed={color === value}
                    style={{ background: value }}
                    onClick={() => (tool === 'pen' ? setPenColor(value) : setMarkColor(value))}
                  />
                ))}
              </div>
            </div>
          )}

          {tool === 'eraser' && (
            <div className="ink">
              <span className="hint">지나간 자리의 획이 지워집니다</span>
              <button
                className="secondary"
                disabled={strokes.length === 0}
                onClick={() => setStrokes([])}
              >
                전부 지우기
              </button>
            </div>
          )}
        </>
      )}
    </div>
  );
}

/* 그리는 칸 ---------------------------------------------------------- */

/** 두께 눈금(1~10)을 칸 너비에 대한 비율로. 형광펜은 같은 눈금에서 훨씬 굵다. */
const widthOf = (step: number, highlighter: boolean) => (highlighter ? 0.012 : 0.003) * step;

/**
 * 손가락으로 긋는 칸.
 *
 * 획이 그어지는 동안은 캔버스에만 그리고, 손을 뗄 때 한 번 부모에게 넘긴다. 점 하나마다
 * 상태를 올리면 그때마다 게임 전체가 저장소로 나가는데, 그건 선 하나 긋는 사이에 수백
 * 번이다.
 *
 * `touch-action: none`이 없으면 손가락을 움직이는 순간 브라우저가 그걸 스크롤로 가져간다 —
 * 그리려던 선은 화면 스크롤이 되고 캔버스에는 점 하나만 남는다.
 */
function Sketch({
  strokes,
  tool,
  color,
  width,
  onDraw,
  onErase,
}: {
  strokes: Stroke[];
  tool: Tool;
  color: string;
  width: number;
  onDraw: (stroke: Ink) => void;
  onErase: (keep: Stroke[]) => void;
}) {
  const canvas = useRef<HTMLCanvasElement | null>(null);
  const live = useRef<Ink | null>(null);
  const all = useRef<Stroke[]>(strokes);
  all.current = strokes;

  const paint = useCallback(() => {
    const element = canvas.current;
    const context = element?.getContext('2d');
    if (!element || !context) return;

    const ratio = window.devicePixelRatio || 1;
    const box = element.getBoundingClientRect();
    if (element.width !== Math.round(box.width * ratio)) {
      element.width = Math.round(box.width * ratio);
      element.height = Math.round(box.height * ratio);
    }

    context.setTransform(ratio, 0, 0, ratio, 0, 0);
    context.clearRect(0, 0, box.width, box.height);
    context.lineCap = 'round';
    context.lineJoin = 'round';

    const lines = live.current ? [...all.current, live.current] : all.current;
    for (const stroke of lines) {
      const ink = inkOf(stroke);
      if (ink.p.length < 2) continue;
      context.globalAlpha = ink.h ? 0.35 : 1;
      context.strokeStyle = ink.c;
      context.lineWidth = Math.max(1, ink.w * box.width);
      context.beginPath();
      context.moveTo(ink.p[0] * box.width, ink.p[1] * box.height);
      for (let i = 2; i < ink.p.length; i += 2) {
        context.lineTo(ink.p[i] * box.width, ink.p[i + 1] * box.height);
      }
      // 점 하나만 찍은 획도 보이게 — 길이가 0인 선은 아무것도 남기지 않는다.
      if (ink.p.length === 2) context.lineTo(ink.p[0] * box.width + 0.1, ink.p[1] * box.height);
      context.stroke();
    }
    context.globalAlpha = 1;
  }, []);

  useEffect(() => {
    paint();
    // 화면이 돌아가면 칸의 크기가 바뀐다. 좌표가 정규화되어 있으므로 다시 그리면 된다.
    window.addEventListener('resize', paint);
    return () => window.removeEventListener('resize', paint);
  }, [paint, strokes]);

  const point = (event: React.PointerEvent<HTMLCanvasElement>): [number, number] => {
    const box = event.currentTarget.getBoundingClientRect();
    return [
      Math.min(1, Math.max(0, (event.clientX - box.left) / box.width)),
      Math.min(1, Math.max(0, (event.clientY - box.top) / box.height)),
    ];
  };

  /**
   * 지우개가 지나간 자리의 획을 통째로 지운다.
   *
   * 획 단위인 것은 종이 지우개와 다르지만, 손가락 굵기로 픽셀을 지우면 남은 선이
   * 너덜너덜해진다. 손글씨 앱들이 "획 지우개"를 기본으로 두는 이유도 같다.
   */
  const eraseAt = (x: number, y: number) => {
    const near = 0.035;
    const keep = all.current.filter((stroke) => {
      const ink = inkOf(stroke);
      for (let i = 0; i < ink.p.length; i += 2) {
        const dx = ink.p[i] - x;
        // 칸이 4:3이라 세로 한 칸이 가로 한 칸보다 짧다. 거리도 그만큼 눕혀야 원이 된다.
        const dy = (ink.p[i + 1] - y) * 0.75;
        if (dx * dx + dy * dy < near * near) return false;
      }
      return true;
    });
    if (keep.length === all.current.length) return;
    all.current = keep;
    onErase(keep);
    paint();
  };

  return (
    <canvas
      ref={canvas}
      className="sketch"
      onPointerDown={(event) => {
        event.currentTarget.setPointerCapture(event.pointerId);
        const [x, y] = point(event);
        if (tool === 'eraser') return eraseAt(x, y);
        if (tool === 'text') return;
        live.current = {
          p: [x, y],
          c: color,
          w: widthOf(width, tool === 'highlighter'),
          h: tool === 'highlighter' || undefined,
        };
        paint();
      }}
      onPointerMove={(event) => {
        const [x, y] = point(event);
        if (tool === 'eraser') {
          if (event.buttons === 0) return;
          return eraseAt(x, y);
        }
        const stroke = live.current;
        if (!stroke) return;
        // 같은 자리에서 떨리는 손가락이 점을 수백 개 만들지 않도록 최소 간격을 둔다.
        const dx = x - stroke.p[stroke.p.length - 2];
        const dy = y - stroke.p[stroke.p.length - 1];
        if (dx * dx + dy * dy < 0.00002) return;
        stroke.p.push(x, y);
        paint();
      }}
      onPointerUp={() => {
        const stroke = live.current;
        live.current = null;
        if (stroke && stroke.p.length >= 2) onDraw(stroke);
        else paint();
      }}
      onPointerCancel={() => {
        live.current = null;
        paint();
      }}
    />
  );
}
