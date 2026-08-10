'use client';

import { useCallback, useEffect, useRef, useState } from 'react';

import type { NotePage, Stroke } from '../../lib/game';

/**
 * 이 판의 노트.
 *
 * 당구장에서 종이에 적는 것들이 있다. 오늘의 내기 조건, 세 명이 돌아가며 칠 때의 순서,
 * 방금 놓친 배치. 앞의 둘은 글이고 마지막은 그림이라, 둘 다 없으면 결국 폰 옆에 종이가
 * 놓인다.
 *
 * 장을 여러 개 두고 위에 탭으로 세운 이유가 그것이다 — 성격이 다른 것들을 한 장에
 * 밀어 넣으면 다음에 찾을 때 스크롤을 하게 된다. 새 장은 만들 때 글인지 그림인지
 * 고르고, 그 뒤로는 바뀌지 않는다.
 *
 * 그린 것은 점의 목록으로 남는다(`lib/game.ts`의 `Stroke`). 좌표가 0~1로 정규화되어
 * 있으므로 폰에서 그린 것이 태블릿에서도, 기록 화면의 작은 칸에서도 같은 그림이다.
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
  const [at, setAt] = useState(0);
  const current = pages[at];

  const replace = (page: NotePage) => onChange(pages.map((entry, index) => (index === at ? page : entry)));

  const add = (kind: NotePage['kind']) => {
    const id = `n-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 6)}`;
    const page: NotePage = kind === 'text' ? { id, kind, text: '' } : { id, kind, strokes: [] };
    onChange([...pages, page]);
    setAt(pages.length);
  };

  const remove = () => {
    const next = pages.filter((_, index) => index !== at);
    onChange(next);
    setAt(Math.max(0, Math.min(at, next.length - 1)));
  };

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

        {/*
          장 목록.
          번호 옆의 기호가 글인지 그림인지 말한다 — 다섯 장쯤 되면 번호만으로는 어느
          장에 무엇이 있었는지 기억나지 않는다.
        */}
        <div className="tabs" role="tablist">
          {pages.map((page, index) => (
            <button
              key={page.id}
              role="tab"
              aria-selected={index === at}
              className="tab"
              onClick={() => setAt(index)}
            >
              {page.kind === 'text' ? '글' : '그림'} {index + 1}
            </button>
          ))}
          <button className="tab add" onClick={() => add('text')} aria-label="글 장 추가">
            + 글
          </button>
          <button className="tab add" onClick={() => add('draw')} aria-label="그림 장 추가">
            + 그림
          </button>
        </div>

        {!current && (
          <p className="notice">
            아직 아무것도 없습니다. 위의 <strong>+ 글</strong>이나 <strong>+ 그림</strong>으로 한 장
            만드세요. 여기 적은 것은 이 판의 기록에 함께 남습니다.
          </p>
        )}

        {current?.kind === 'text' && (
          <textarea
            className="note-text"
            value={current.text}
            placeholder="내기 조건, 순서, 무엇이든"
            onChange={(event) => replace({ ...current, text: event.target.value })}
          />
        )}

        {current?.kind === 'draw' && (
          <DrawPad
            strokes={current.strokes}
            onChange={(strokes) => replace({ ...current, strokes })}
          />
        )}

        {current && (
          <button className="ghost" onClick={remove}>
            이 장 지우기
          </button>
        )}
      </div>
    </div>
  );
}

/* 그림 --------------------------------------------------------------- */

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
function DrawPad({ strokes, onChange }: { strokes: Stroke[]; onChange: (strokes: Stroke[]) => void }) {
  const canvas = useRef<HTMLCanvasElement | null>(null);
  const drawing = useRef<Stroke | null>(null);
  const all = useRef<Stroke[]>(strokes);

  all.current = strokes;

  /** 지금까지의 획 전부를 다시 그린다. 크기가 바뀌었을 때도 이 하나면 된다. */
  const repaint = useCallback(() => {
    const element = canvas.current;
    const context = element?.getContext('2d');
    if (!element || !context) return;

    const ratio = window.devicePixelRatio || 1;
    const width = element.clientWidth;
    const height = element.clientHeight;
    if (element.width !== Math.round(width * ratio)) {
      element.width = Math.round(width * ratio);
      element.height = Math.round(height * ratio);
    }

    context.setTransform(ratio, 0, 0, ratio, 0, 0);
    context.clearRect(0, 0, width, height);
    context.lineWidth = 2.5;
    context.lineCap = 'round';
    context.lineJoin = 'round';
    context.strokeStyle = '#f3f4f6';

    const lines = drawing.current ? [...all.current, drawing.current] : all.current;
    for (const stroke of lines) {
      if (stroke.length < 2) continue;
      context.beginPath();
      context.moveTo(stroke[0] * width, stroke[1] * height);
      for (let i = 2; i < stroke.length; i += 2) context.lineTo(stroke[i] * width, stroke[i + 1] * height);
      // 점 하나만 찍은 획도 보이게 — 선으로는 길이가 0이라 아무것도 남지 않는다.
      if (stroke.length === 2) context.lineTo(stroke[0] * width + 0.1, stroke[1] * height);
      context.stroke();
    }
  }, []);

  useEffect(() => {
    repaint();
    // 화면이 돌아가면 캔버스 크기가 바뀐다. 정규화된 좌표라 다시 그리기만 하면 된다.
    window.addEventListener('resize', repaint);
    return () => window.removeEventListener('resize', repaint);
  }, [repaint, strokes]);

  const point = (event: React.PointerEvent<HTMLCanvasElement>) => {
    const box = event.currentTarget.getBoundingClientRect();
    return [
      Math.min(1, Math.max(0, (event.clientX - box.left) / box.width)),
      Math.min(1, Math.max(0, (event.clientY - box.top) / box.height)),
    ];
  };

  return (
    <div className="sketch-wrap">
      <canvas
        ref={canvas}
        className="sketch"
        onPointerDown={(event) => {
          event.currentTarget.setPointerCapture(event.pointerId);
          drawing.current = point(event);
          repaint();
        }}
        onPointerMove={(event) => {
          if (!drawing.current) return;
          const [x, y] = point(event);
          const last = drawing.current;
          // 같은 자리에서 떨리는 손가락이 점을 수백 개 만들지 않도록 최소 간격을 둔다.
          const dx = x - last[last.length - 2];
          const dy = y - last[last.length - 1];
          if (dx * dx + dy * dy < 0.00002) return;
          last.push(x, y);
          repaint();
        }}
        onPointerUp={() => {
          const stroke = drawing.current;
          drawing.current = null;
          if (stroke && stroke.length >= 2) onChange([...all.current, stroke]);
          else repaint();
        }}
        onPointerCancel={() => {
          drawing.current = null;
          repaint();
        }}
      />
      <div className="row">
        <button
          className="secondary"
          disabled={strokes.length === 0}
          onClick={() => onChange(strokes.slice(0, -1))}
        >
          한 획 지우기
        </button>
        <button className="secondary" disabled={strokes.length === 0} onClick={() => onChange([])}>
          전부 지우기
        </button>
      </div>
    </div>
  );
}

/* 다시 보기 ---------------------------------------------------------- */

/**
 * 그린 것을 다시 보여 주기만 하는 그림.
 *
 * 기록 화면에서 쓴다. 캔버스가 아니라 SVG인 이유는 크기가 정해지지 않은 자리에 들어가기
 * 때문이다 — SVG는 `viewBox`가 알아서 늘어나고 줄어들지만, 캔버스는 픽셀 수를 누군가
 * 계산해서 넣어야 한다.
 */
export function StrokesView({ strokes }: { strokes: Stroke[] }) {
  return (
    <svg className="strokes" viewBox="0 0 100 100" preserveAspectRatio="none" aria-label="그림 메모">
      {strokes.map((stroke, index) => {
        const points: string[] = [];
        for (let i = 0; i < stroke.length; i += 2) {
          points.push(`${(stroke[i] * 100).toFixed(2)},${(stroke[i + 1] * 100).toFixed(2)}`);
        }
        return (
          <polyline
            key={index}
            points={points.join(' ')}
            fill="none"
            stroke="currentColor"
            strokeWidth="0.6"
            strokeLinecap="round"
            strokeLinejoin="round"
            vectorEffect="non-scaling-stroke"
          />
        );
      })}
    </svg>
  );
}
