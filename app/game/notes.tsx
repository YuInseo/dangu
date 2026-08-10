'use client';

import { useCallback, useEffect, useRef, useState } from 'react';

import { inkOf, type Ball, type Ink, type NotePage, type Stroke } from '../../lib/game';

/**
 * 이 판의 노트.
 *
 * 당구장에서 종이에 적는 것들이 있다. 오늘의 내기 조건, 셋이 돌아가며 칠 때의 순서,
 * 방금 놓친 배치. 앞의 둘은 글이고 마지막은 그림인데 대개 한 장에 같이 적힌다 —
 * 순서를 적고 그 옆에 자리를 그린다. 그래서 한 장이 글과 그림을 함께 갖는다.
 *
 * 생김새는 손글씨 앱들의 것을 따랐다. 도구는 화면 맨 아래 한 줄의 아이콘이고, 지금 든
 * 도구를 한 번 더 누르면 그 도구의 설정이 줄 위로 떠오른다. 이미 손에 익은 배치를
 * 굳이 새로 배우게 할 이유가 없다.
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

/* 도구 --------------------------------------------------------------- */

const PEN_COLORS = ['#ef4444', '#fbbf24', '#facc15', '#34d399', '#38bdf8', '#3b82f6', '#a855f7', '#f3f4f6', '#111827'];
const HIGHLIGHT_COLORS = ['#ef4444', '#fbbf24', '#facc15', '#34d399', '#38bdf8', '#3b82f6', '#a855f7', '#f3f4f6'];

type Tool = 'text' | 'pen' | 'highlighter' | 'eraser' | 'ball';

/**
 * 공의 색. 당구공은 셋뿐이라 고르는 것도 셋이면 된다.
 *
 * `max`는 판에 실제로 있는 개수다 — 흰 공과 노란 공이 하나씩, 빨간 공이 둘(4구). 없는
 * 공을 놓을 수 있게 두면 그건 당구대가 아니라 그냥 초록 종이다.
 */
const BALLS: { id: Ball['c']; label: string; fill: string; edge: string; max: number }[] = [
  { id: 'w', label: '흰 공', fill: '#f7f7f4', edge: 'rgba(0,0,0,0.35)', max: 1 },
  { id: 'y', label: '노란 공', fill: '#ffd43a', edge: 'rgba(0,0,0,0.35)', max: 1 },
  { id: 'r', label: '빨간 공', fill: '#e0322c', edge: 'rgba(0,0,0,0.35)', max: 2 },
];

const limitOf = (color: Ball['c']) => BALLS.find((entry) => entry.id === color)?.max ?? 1;

/**
 * 펜촉.
 *
 * 굵기 눈금은 같아도 촉마다 다른 굵기가 나온다 — 만년필이 제일 굵고 연필이 제일 가늘다.
 * 실제 앱의 촉들이 필압과 질감까지 다르지만, 손가락으로 긋는 화면에서 필압은 없는
 * 값이므로 굵기와 진하기만 다르게 둔다. 없는 것을 흉내 내는 것보다 있는 것을 정직하게.
 */
const NIBS = [
  { id: 'fountain', label: '만년필', scale: 1.35, alpha: 1 },
  { id: 'pen', label: '볼펜', scale: 1, alpha: 1 },
  { id: 'pencil', label: '연필', scale: 0.7, alpha: 0.75 },
] as const;

type NibId = (typeof NIBS)[number]['id'];

/**
 * 키보드가 가린 높이를 CSS에 알려 준다.
 *
 * 안드로이드 WebView는 키보드가 떠도 화면 크기를 늘 줄여 주지 않는다. 그러면 글을 적는
 * 동안 도구 줄이 키보드 뒤에 숨어서, 쓰다가 펜으로 바꾸려면 키보드를 먼저 내려야 한다.
 * `visualViewport`는 실제로 보이는 만큼을 알려 주므로, 그 차이를 `--kb`로 내보내고 시트가
 * 그만큼 올라선다.
 */
function useKeyboardInset() {
  useEffect(() => {
    const view = window.visualViewport;
    if (!view) return;

    const apply = () => {
      const hidden = Math.max(0, window.innerHeight - view.height - view.offsetTop);
      // 1px씩 흔들리는 값으로 레이아웃을 다시 잡지 않도록 아주 작은 값은 0으로 본다.
      document.documentElement.style.setProperty('--kb', `${hidden > 24 ? Math.round(hidden) : 0}px`);
    };

    apply();
    view.addEventListener('resize', apply);
    view.addEventListener('scroll', apply);
    return () => {
      view.removeEventListener('resize', apply);
      view.removeEventListener('scroll', apply);
      document.documentElement.style.removeProperty('--kb');
    };
  }, []);
}

export function NotePages({
  pages,
  onChange,
}: {
  pages: NotePage[];
  onChange: (pages: NotePage[]) => void;
}) {
  useKeyboardInset();
  const [at, setAt] = useState(0);
  const [tool, setTool] = useState<Tool>('pen');
  /** 도구 설정 카드가 떠 있는지. 든 도구를 한 번 더 누르면 열린다. */
  const [open, setOpen] = useState(false);

  const [nib, setNib] = useState<NibId>('pen');
  const [thick, setThick] = useState(50);
  const [penColor, setPenColor] = useState(PEN_COLORS[7]);
  const [straight, setStraight] = useState(false);

  const [markThick, setMarkThick] = useState(19);
  const [markAlpha, setMarkAlpha] = useState(100);
  const [markColor, setMarkColor] = useState(HIGHLIGHT_COLORS[2]);

  const [areaEraser, setAreaEraser] = useState(false);
  const [markOnly, setMarkOnly] = useState(false);

  const [ballColor, setBallColor] = useState<Ball['c']>('w');

  /** 되돌린 획들. 저장하지 않는다 — 노트를 다시 열 때까지 남을 성질의 것이 아니다. */
  const [undone, setUndone] = useState<Stroke[]>([]);

  const index = Math.min(at, Math.max(0, pages.length - 1));
  const current = pages[index];
  const strokes = current?.strokes ?? [];
  const balls = current?.balls ?? [];

  const replace = (page: NotePage) =>
    onChange(pages.map((entry, position) => (position === index ? page : entry)));

  const setStrokes = (next: Stroke[]) => {
    if (!current) return;
    replace({ ...current, strokes: next });
  };

  const setBalls = (next: Ball[]) => {
    if (!current) return;
    replace({ ...current, balls: next });
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

  /** 도구를 누른다. 이미 든 것을 또 누르면 설정이 열린다 — 손글씨 앱들의 규칙이다. */
  const pick = (next: Tool) => {
    if (next === tool) setOpen((was) => !was);
    else {
      setTool(next);
      setOpen(false);
    }
  };

  // 노트를 연 사람은 적으려고 연 것이다. 빈 노트에 "먼저 장을 만드세요"는 이유 없는 한 걸음이다.
  useEffect(() => {
    if (pages.length === 0) add();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [pages.length]);

  const nibOf = NIBS.find((entry) => entry.id === nib) ?? NIBS[1];

  return (
    <div className="notes-body">
      {/* 장 고르기와, 그 옆의 더하기·지우기. 동작은 탭이 아니므로 홈 밖에 선다. */}
      <div className="tabs-row">
        {pages.length > 0 && (
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
          </div>
        )}
        <button className="icon-round" onClick={add} aria-label="장 추가">
          +
        </button>
        {pages.length > 0 && (
          <button className="icon-round" onClick={remove} aria-label="이 장 지우기">
            ✕
          </button>
        )}
      </div>

      {pages.length === 0 && (
        <p className="notice">
          <strong>+</strong>를 눌러 시작하세요. 여기 적은 것은 이 판의 기록에 함께 남습니다.
        </p>
      )}

      {current && (
        <>
          {/*
            한 장은 한 판이다.

            글칸과 그림칸을 따로 두었더니 "여기에 적고 저기에 그리는" 화면이 되었다.
            실제로 적는 것은 그렇지 않다 — 배치를 그려 놓고 그 옆에 조건을 적는다.
            그래서 셋을 같은 자리에 겹쳤다: 아래에서부터 당구대, 글, 그리고 그림.

            글 도구를 들면 글칸이 위로 올라와 손가락을 받고, 그리는 도구를 들면 그림이
            위로 올라온다. 보이는 것은 늘 셋 다이고, 무엇을 만지는지만 도구가 정한다.
          */}
          <div className={`paper${current.plain ? '' : ' table'}`} data-tool={tool}>
            <textarea
              className="note-text"
              value={current.text ?? ''}
              placeholder={tool === 'text' ? '내기 조건, 순서, 무엇이든' : ''}
              onChange={(event) => replace({ ...current, text: event.target.value })}
            />

            <Sketch
              strokes={strokes}
              balls={balls}
              tool={tool}
              ballColor={ballColor}
              color={tool === 'highlighter' ? markColor : penColor}
              width={
                tool === 'highlighter'
                  ? (markThick / 100) * 0.09 + 0.01
                  : ((thick / 100) * 0.02 + 0.002) * nibOf.scale
              }
              alpha={tool === 'highlighter' ? markAlpha / 100 : nibOf.alpha}
              highlighter={tool === 'highlighter'}
              straight={straight}
              areaEraser={areaEraser}
              markOnly={markOnly}
              onDraw={(stroke) => {
                setStrokes([...strokes, stroke]);
                setUndone([]);
              }}
              onErase={(keep) => {
                setStrokes(keep);
                setUndone([]);
              }}
              onBalls={setBalls}
            />

            <span className="page-no">
              {index + 1}/{pages.length}
            </span>
          </div>

          {/* 도구 설정. 도구 줄 위로 떠오른다. */}
          {open && tool === 'pen' && (
            <Popup onClose={() => setOpen(false)}>
              <div className="nibs">
                {NIBS.map((entry) => (
                  <button
                    key={entry.id}
                    className="nib"
                    aria-pressed={nib === entry.id}
                    onClick={() => setNib(entry.id)}
                  >
                    <NibIcon id={entry.id} color={penColor} />
                    <span>{entry.label}</span>
                  </button>
                ))}
              </div>
              <hr className="pop-line" />
              <Slider value={thick} onChange={setThick} label="두께" />
              <Toggle label="직선 자동 보정" on={straight} onChange={setStraight} />
              <Swatches colors={PEN_COLORS} value={penColor} onChange={setPenColor} />
            </Popup>
          )}

          {open && tool === 'highlighter' && (
            <Popup onClose={() => setOpen(false)}>
              <Slider value={markThick} onChange={setMarkThick} label="두께" />
              <Slider value={markAlpha} onChange={setMarkAlpha} label="진하기" track="alpha" color={markColor} />
              <Swatches colors={HIGHLIGHT_COLORS} value={markColor} onChange={setMarkColor} />
            </Popup>
          )}

          {open && tool === 'ball' && (
            <Popup title="공 놓기" onClose={() => setOpen(false)}>
              <div className="ball-picks">
                {BALLS.map((ball) => (
                  <button
                    key={ball.id}
                    className="ball-pick"
                    aria-pressed={ballColor === ball.id}
                    onClick={() => setBallColor(ball.id)}
                  >
                    <span className="dot-ball" style={{ background: ball.fill }} />
                    <span>
                      {ball.label} {balls.filter((entry) => entry.c === ball.id).length}/{ball.max}
                    </span>
                  </button>
                ))}
              </div>
              <p className="hint">
                판을 누르면 놓이고, 공을 끌면 옮겨집니다. 흰 공과 노란 공은 하나씩, 빨간 공은
                둘까지 — 다 나와 있으면 누른 자리로 옮겨집니다.
              </p>
              <Toggle
                label="당구대 배경"
                on={!current.plain}
                onChange={(on) => replace({ ...current, plain: on ? undefined : true })}
              />
              <hr className="pop-line dotted" />
              <div className="row">
                <button
                  className="secondary"
                  disabled={balls.length === 0}
                  onClick={() => setBalls(balls.slice(0, -1))}
                >
                  마지막 공 빼기
                </button>
                <button className="secondary" disabled={balls.length === 0} onClick={() => setBalls([])}>
                  공 전부 빼기
                </button>
              </div>
            </Popup>
          )}

          {open && tool === 'eraser' && (
            <Popup title="손글씨 지우개" onClose={() => setOpen(false)}>
              <Radio label="획 지우개" on={!areaEraser} onPick={() => setAreaEraser(false)} />
              <Radio label="영역 지우개" on={areaEraser} onPick={() => setAreaEraser(true)} />
              <Toggle label="형광펜만 지우기" on={markOnly} onChange={setMarkOnly} />
              <hr className="pop-line dotted" />
              <button
                className="pop-wide"
                disabled={strokes.length === 0}
                onClick={() => {
                  setStrokes([]);
                  setUndone([]);
                }}
              >
                손글씨 모두 지우기
              </button>
            </Popup>
          )}

          {/*
            도구 줄.

            그리는 손이 이미 화면 아래에 있으므로 도구도 거기 있어야 한다. 든 도구는
            동그란 배경으로 눈에 띄고, 한 번 더 누르면 설정이 열린다.
          */}
          <div className="tools">
            <button
              className="tool round"
              aria-pressed={tool === 'text'}
              aria-label="글"
              onClick={() => pick('text')}
            >
              <KeyboardIcon />
            </button>
            <button className="tool" aria-pressed={tool === 'pen'} aria-label="펜" onClick={() => pick('pen')}>
              <PenIcon color={penColor} />
            </button>
            <button
              className="tool"
              aria-pressed={tool === 'highlighter'}
              aria-label="형광펜"
              onClick={() => pick('highlighter')}
            >
              <MarkerIcon color={markColor} />
            </button>
            <button
              className="tool"
              aria-pressed={tool === 'eraser'}
              aria-label="지우개"
              onClick={() => pick('eraser')}
            >
              <EraserIcon />
            </button>
            <button
              className="tool"
              aria-pressed={tool === 'ball'}
              aria-label="공 놓기"
              onClick={() => pick('ball')}
            >
              <BallIcon color={BALLS.find((entry) => entry.id === ballColor)?.fill ?? '#fff'} />
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
              <UndoIcon />
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
              <UndoIcon flip />
            </button>
          </div>
        </>
      )}
    </div>
  );
}

/* 설정 카드의 부품들 --------------------------------------------------- */

function Popup({
  title,
  children,
  onClose,
}: {
  title?: string;
  children: React.ReactNode;
  onClose: () => void;
}) {
  return (
    <div className="ink-pop">
      <div className="pop-head">
        {title ? <strong>{title}</strong> : <span className="fav">☆</span>}
        <button className="pop-x" onClick={onClose} aria-label="닫기">
          ✕
        </button>
      </div>
      {children}
    </div>
  );
}

/**
 * 굵기·진하기 막대.
 *
 * 양 끝의 −와 +가 있는 이유는 손가락으로 한 눈금씩 옮기는 게 막대를 정확히 잡는 것보다
 * 쉽기 때문이다. 진하기 막대의 바탕이 체크무늬인 것은 그 값이 투명도라는 걸 말한다.
 */
function Slider({
  value,
  onChange,
  label,
  track,
  color,
}: {
  value: number;
  onChange: (value: number) => void;
  label: string;
  track?: 'alpha';
  color?: string;
}) {
  const step = (delta: number) => onChange(Math.min(100, Math.max(1, value + delta)));
  return (
    <div className="slider-row">
      <button className="step" onClick={() => step(-10)} aria-label={`${label} 줄이기`}>
        −
      </button>
      <span
        className={`track${track === 'alpha' ? ' alpha' : ''}`}
        style={track === 'alpha' ? ({ '--ink': color } as React.CSSProperties) : undefined}
      >
        <input
          type="range"
          min={1}
          max={100}
          value={value}
          aria-label={label}
          onChange={(event) => onChange(Number(event.target.value))}
        />
        <span className="bubble" style={{ left: `${value}%` }}>
          {value}
        </span>
      </span>
      <button className="step" onClick={() => step(10)} aria-label={`${label} 늘리기`}>
        +
      </button>
    </div>
  );
}

function Toggle({
  label,
  on,
  onChange,
}: {
  label: string;
  on: boolean;
  onChange: (on: boolean) => void;
}) {
  return (
    <button className="switch-row" role="switch" aria-checked={on} onClick={() => onChange(!on)}>
      <span>{label}</span>
      <span className="switch" />
    </button>
  );
}

function Radio({ label, on, onPick }: { label: string; on: boolean; onPick: () => void }) {
  return (
    <button className="radio-row" role="radio" aria-checked={on} onClick={onPick}>
      <span className="dot" />
      <span>{label}</span>
    </button>
  );
}

function Swatches({
  colors,
  value,
  onChange,
}: {
  colors: readonly string[];
  value: string;
  onChange: (color: string) => void;
}) {
  return (
    <div className="swatches">
      {colors.map((color) => (
        <button
          key={color}
          className="swatch"
          aria-label={`색 ${color}`}
          aria-pressed={value === color}
          style={{ background: color }}
          onClick={() => onChange(color)}
        >
          {value === color ? <CheckIcon /> : null}
        </button>
      ))}
    </div>
  );
}

/* 아이콘 -------------------------------------------------------------- */

const KeyboardIcon = () => (
  <svg viewBox="0 0 24 24" aria-hidden>
    <rect x="2.5" y="6" width="19" height="12" rx="2.5" fill="none" stroke="currentColor" strokeWidth="1.6" />
    <path
      d="M6 10h1M9 10h1M12 10h1M15 10h1M18 10h1M6 13h1M9 13h1M12 13h1M15 13h1M18 13h1M8 15.6h8"
      stroke="currentColor"
      strokeWidth="1.6"
      strokeLinecap="round"
    />
  </svg>
);

const PenIcon = ({ color }: { color: string }) => (
  <svg viewBox="0 0 24 24" aria-hidden>
    <path d="M5 19l1.4-4.2L16.2 5l2.8 2.8-9.8 9.8L5 19z" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinejoin="round" />
    <path d="M5 19l1.4-4.2 2.8 2.8L5 19z" fill={color} stroke="none" />
  </svg>
);

const MarkerIcon = ({ color }: { color: string }) => (
  <svg viewBox="0 0 24 24" aria-hidden>
    <path d="M7.5 15.5l6.8-9.2 3.4 2.6-6.6 9.4-3.9.6.3-3.4z" fill={color} stroke="none" opacity="0.9" />
    <path d="M4.5 20.5h15" stroke={color} strokeWidth="2.4" strokeLinecap="round" opacity="0.6" />
  </svg>
);

const EraserIcon = () => (
  <svg viewBox="0 0 24 24" aria-hidden>
    <path
      d="M9.5 19.5H19M4.6 15.2l6-6a2 2 0 012.8 0l4 4a2 2 0 010 2.8l-3.5 3.5H8.1L4.6 18a2 2 0 010-2.8z"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.6"
      strokeLinejoin="round"
      strokeLinecap="round"
    />
  </svg>
);

const UndoIcon = ({ flip }: { flip?: boolean }) => (
  <svg viewBox="0 0 24 24" aria-hidden style={flip ? { transform: 'scaleX(-1)' } : undefined}>
    <path
      d="M9 7H14.5a4.5 4.5 0 010 9H8M9 7L5.5 4M9 7L5.5 10"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.8"
      strokeLinecap="round"
      strokeLinejoin="round"
    />
  </svg>
);

const BallIcon = ({ color }: { color: string }) => (
  <svg viewBox="0 0 24 24" aria-hidden>
    <circle cx="9" cy="14" r="5" fill={color} stroke="rgba(0,0,0,0.35)" strokeWidth="0.8" />
    <circle cx="16" cy="9" r="5" fill="#e0322c" stroke="rgba(0,0,0,0.35)" strokeWidth="0.8" />
  </svg>
);

const CheckIcon = () => (
  <svg viewBox="0 0 24 24" aria-hidden className="check">
    <path d="M5 12.5l4.5 4.5L19 7.5" fill="none" stroke="#16181d" strokeWidth="2.6" strokeLinecap="round" strokeLinejoin="round" />
  </svg>
);

const NibIcon = ({ id, color }: { id: NibId; color: string }) => {
  if (id === 'pencil') {
    return (
      <svg viewBox="0 0 24 40" aria-hidden>
        <path d="M12 3l4 7v24a2 2 0 01-2 2h-4a2 2 0 01-2-2V10l4-7z" fill="#b45309" />
        <path d="M12 3l4 7H8l4-7z" fill={color} />
      </svg>
    );
  }
  if (id === 'fountain') {
    return (
      <svg viewBox="0 0 24 40" aria-hidden>
        <path d="M12 2l5 9v23a2 2 0 01-2 2H9a2 2 0 01-2-2V11l5-9z" fill="#374151" />
        <path d="M12 2l5 9H7l5-9z" fill={color} />
      </svg>
    );
  }
  return (
    <svg viewBox="0 0 24 40" aria-hidden>
      <path d="M8 6h8v28a2 2 0 01-2 2h-4a2 2 0 01-2-2V6z" fill="#4b5563" />
      <path d="M8 6h8v5H8z" fill={color} />
    </svg>
  );
};

/* 그리는 칸 ---------------------------------------------------------- */

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
  balls,
  tool,
  ballColor,
  color,
  width,
  alpha,
  highlighter,
  straight,
  areaEraser,
  markOnly,
  onDraw,
  onErase,
  onBalls,
}: {
  strokes: Stroke[];
  balls: Ball[];
  tool: Tool;
  ballColor: Ball['c'];
  color: string;
  width: number;
  alpha: number;
  highlighter: boolean;
  straight: boolean;
  areaEraser: boolean;
  markOnly: boolean;
  onDraw: (stroke: Ink) => void;
  onErase: (keep: Stroke[]) => void;
  onBalls: (balls: Ball[]) => void;
}) {
  const canvas = useRef<HTMLCanvasElement | null>(null);
  const live = useRef<Ink | null>(null);
  const all = useRef<Stroke[]>(strokes);
  all.current = strokes;
  const kept = useRef<Ball[]>(balls);
  kept.current = balls;
  /** 끌고 있는 공의 자리. 손을 뗄 때 한 번만 저장한다 — 획과 같은 이유다. */
  const dragging = useRef<number | null>(null);

  /** 공의 반지름 — 칸 너비에 대한 비율. 진짜 비율(2.2%)보다 조금 크게 그려야 손에 잡힌다. */
  const RADIUS = 0.032;

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
      context.globalAlpha = ink.a ?? (ink.h ? 0.35 : 1);
      context.strokeStyle = ink.c;
      // 형광펜은 끝이 각진 편이 그은 자리가 또렷하다.
      context.lineCap = ink.h ? 'butt' : 'round';
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

    /*
     * 공은 획 위에 그린다.
     *
     * 판 위에 놓인 물건이므로 그어 놓은 선 아래로 숨으면 안 된다. 위쪽에 옅은 흰빛을
     * 얹어 두는데, 그 한 점이 공을 동그란 물체로 보이게 한다 — 납작한 원과 공의 차이는
     * 대개 그 반사광 하나다.
     */
    const radius = RADIUS * box.width;
    for (const ball of kept.current) {
      const spec = BALLS.find((entry) => entry.id === ball.c) ?? BALLS[0];
      const x = ball.x * box.width;
      const y = ball.y * box.height;

      context.beginPath();
      context.arc(x, y, radius, 0, Math.PI * 2);
      context.fillStyle = spec.fill;
      context.fill();
      context.lineWidth = Math.max(1, radius * 0.12);
      context.strokeStyle = spec.edge;
      context.stroke();

      context.beginPath();
      context.arc(x - radius * 0.3, y - radius * 0.35, radius * 0.28, 0, Math.PI * 2);
      context.fillStyle = 'rgba(255, 255, 255, 0.55)';
      context.fill();
    }
  }, []);

  useEffect(() => {
    paint();
    // 화면이 돌아가면 칸의 크기가 바뀐다. 좌표가 정규화되어 있으므로 다시 그리면 된다.
    window.addEventListener('resize', paint);
    return () => window.removeEventListener('resize', paint);
  }, [paint, strokes, balls]);

  const point = (event: React.PointerEvent<HTMLCanvasElement>): [number, number] => {
    const box = event.currentTarget.getBoundingClientRect();
    return [
      Math.min(1, Math.max(0, (event.clientX - box.left) / box.width)),
      Math.min(1, Math.max(0, (event.clientY - box.top) / box.height)),
    ];
  };

  const erase = (x: number, y: number) => {
    const keep = areaEraser
      ? eraseArea(all.current, x, y, markOnly)
      : eraseStrokes(all.current, x, y, markOnly);
    if (keep.length === all.current.length && keep.every((entry, i) => entry === all.current[i])) return;
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
        if (tool === 'eraser') return erase(x, y);
        if (tool === 'text') return;
        if (tool === 'ball') {
          /*
           * 이미 있는 공을 짚었으면 그걸 옮기고, 빈 자리를 짚었으면 새로 놓는다.
           *
           * "놓기"와 "옮기기"를 도구 두 개로 나누지 않은 이유는 손이 하는 일이 하나이기
           * 때문이다 — 판 위의 공을 손가락으로 민다. 짚은 자리에 공이 있느냐 없느냐로
           * 갈리는 것이 도구를 갈아 드는 것보다 짧다.
           */
          const box = event.currentTarget.getBoundingClientRect();
          const ratio = box.height / box.width;
          const hit = kept.current.findIndex((ball) => {
            const dx = ball.x - x;
            const dy = (ball.y - y) * ratio;
            return dx * dx + dy * dy < RADIUS * RADIUS * 1.6;
          });
          if (hit >= 0) {
            dragging.current = hit;
          } else if (kept.current.filter((ball) => ball.c === ballColor).length >= limitOf(ballColor)) {
            /*
             * 그 색이 이미 다 나와 있으면, 제일 먼저 놓았던 것을 여기로 옮긴다.
             *
             * 아무 일도 일어나지 않게 두는 쪽이 규칙으로는 깔끔하지만, 화면은 누른 손에
             * 답을 해야 한다. 그리고 이 상황에서 사람이 원하는 것은 대개 "그 공을 여기로"다 —
             * 끌어다 놓는 것과 같은 결과를, 한 번 누르는 것으로 준다.
             */
            const at = kept.current.findIndex((ball) => ball.c === ballColor);
            const next = kept.current.map((ball, i) => (i === at ? { ...ball, x, y } : ball));
            kept.current = next;
            dragging.current = at;
            onBalls(next);
          } else {
            const next = [...kept.current, { c: ballColor, x, y }];
            kept.current = next;
            dragging.current = next.length - 1;
            onBalls(next);
          }
          paint();
          return;
        }
        // 해당 없는 값은 키를 아예 만들지 않는다 — `undefined`가 들어 있는 객체는
        // 아일랜드 직렬화 검사에서 터지고, 그러면 노트가 통째로 사라진다.
        const ink: Ink = { p: [x, y], c: color, w: width };
        if (highlighter) ink.h = true;
        if (alpha < 1) ink.a = Number(alpha.toFixed(2));
        live.current = ink;
        paint();
      }}
      onPointerMove={(event) => {
        const [x, y] = point(event);
        if (tool === 'eraser') {
          if (event.buttons === 0) return;
          return erase(x, y);
        }
        if (tool === 'ball') {
          const at = dragging.current;
          if (at === null || event.buttons === 0) return;
          // 끄는 동안은 캔버스에만 옮긴다. 손을 뗄 때 한 번 저장한다.
          kept.current = kept.current.map((ball, i) => (i === at ? { ...ball, x, y } : ball));
          paint();
          return;
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
        if (tool === 'ball') {
          if (dragging.current !== null) onBalls(kept.current);
          dragging.current = null;
          return;
        }
        const stroke = live.current;
        live.current = null;
        if (!stroke || stroke.p.length < 2) return paint();
        onDraw(straight && !highlighter ? straighten(stroke) : stroke);
      }}
      onPointerCancel={() => {
        live.current = null;
        dragging.current = null;
        paint();
      }}
    />
  );
}

/* 획 다루기 ----------------------------------------------------------- */

/** 지우개가 닿은 획을 통째로 버린다. 손글씨 앱의 "획 지우개". */
function eraseStrokes(strokes: Stroke[], x: number, y: number, markOnly: boolean): Stroke[] {
  return strokes.filter((stroke) => {
    const ink = inkOf(stroke);
    if (markOnly && !ink.h) return true;
    return !touches(ink, x, y);
  });
}

/**
 * 닿은 부분만 지운다 — "영역 지우개".
 *
 * 획을 점 단위로 잘라 내고 남은 토막들을 각각 다시 획으로 만든다. 한 획의 가운데를
 * 지우면 두 획이 되는 것이 맞다: 그러지 않으면 다음에 그 획의 다른 끝을 지울 때 이미
 * 지운 부분이 함께 살아 돌아온다.
 */
function eraseArea(strokes: Stroke[], x: number, y: number, markOnly: boolean): Stroke[] {
  const near = 0.045;
  const next: Stroke[] = [];

  for (const stroke of strokes) {
    const ink = inkOf(stroke);
    if ((markOnly && !ink.h) || !touches(ink, x, y)) {
      next.push(stroke);
      continue;
    }

    let piece: number[] = [];
    for (let i = 0; i < ink.p.length; i += 2) {
      const dx = ink.p[i] - x;
      const dy = (ink.p[i + 1] - y) * 0.75;
      if (dx * dx + dy * dy < near * near) {
        if (piece.length >= 4) next.push({ ...ink, p: piece });
        piece = [];
      } else {
        piece.push(ink.p[i], ink.p[i + 1]);
      }
    }
    if (piece.length >= 4) next.push({ ...ink, p: piece });
  }

  return next;
}

/** 획이 이 자리에 닿는지. 칸이 4:3이라 세로 거리를 눕혀야 지우개가 원으로 동작한다. */
function touches(ink: Ink, x: number, y: number): boolean {
  const near = 0.045;
  for (let i = 0; i < ink.p.length; i += 2) {
    const dx = ink.p[i] - x;
    const dy = (ink.p[i + 1] - y) * 0.75;
    if (dx * dx + dy * dy < near * near) return true;
  }
  return false;
}

/**
 * 직선 자동 보정.
 *
 * 그은 선이 두 끝점을 잇는 직선에서 거의 벗어나지 않았으면 진짜 직선으로 바꾼다. 자를
 * 대고 그은 것처럼 보이라는 뜻이 아니라, 손으로 그은 당구대 모서리가 휘어 보이지 않게
 * 하려는 것이다. 많이 휜 선은 그리려던 곡선이므로 건드리지 않는다.
 */
function straighten(ink: Ink): Ink {
  const points = ink.p;
  if (points.length < 6) return ink;

  const [x1, y1] = [points[0], points[1]];
  const [x2, y2] = [points[points.length - 2], points[points.length - 1]];
  const dx = x2 - x1;
  const dy = y2 - y1;
  const length = Math.hypot(dx, dy);
  if (length < 0.05) return ink;

  let worst = 0;
  for (let i = 2; i < points.length - 2; i += 2) {
    const away = Math.abs(dy * (points[i] - x1) - dx * (points[i + 1] - y1)) / length;
    worst = Math.max(worst, away);
  }

  return worst < 0.03 ? { ...ink, p: [x1, y1, x2, y2] } : ink;
}
