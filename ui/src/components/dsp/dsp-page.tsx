'use client';

/**
 * 소리결 — 정밀 오디오 제어. 룬의 MUSE 화면을 따라 만들었다.
 *
 * 왼쪽은 이 존의 신호 흐름: 볼륨 레벨링 · 헤드룸 · 샘플레이트 변환 · 출력, 그 아래 사용자가 순서를
 * 정하는 필터들. 오른쪽은 고른 항목의 편집 패널. 아무것도 고르지 않으면 소개와 지금 곡의 신호 경로.
 */
import { useEffect, useMemo, useRef, useState, type ReactNode } from 'react';
import { FILTER_BLURB, FILTER_LABEL, newFilter } from '@/lib/dsp/defaults';
import { fmtDb } from '@/lib/dsp/plan';
import { SRC_MODE_LABEL } from '@/lib/dsp/src';
import { useDsp } from '@/lib/dsp/store';
import type { DspFilter, FilterType } from '@/lib/dsp/types';
import { useEngineState, usePlayer, useQueue } from '@/lib/player/context';
import { useZoneOutput } from '@/lib/player/zone-output';
import { SIGNAL_LABEL, signalPath } from '@/lib/player/signal';
import { khz } from '@/lib/quality';
import {
  AudioWaveform,
  ChevronDown,
  ChevronLeft,
  ChevronRight,
  Combine,
  GripVertical,
  Headphones,
  Monitor,
  Plus,
  RotateCcw,
  Save,
  SlidersHorizontal,
  Speaker,
  Trash2,
  Waves,
  X,
} from '../icons';
import { QualityGlyph } from '../signal-path';
import {
  ConvolutionEditor,
  CrossfeedEditor,
  EqEditor,
  HeadroomEditor,
  LevelingEditor,
  OutputEditor,
  ProceduralEditor,
  SrcEditor,
  filterBoost,
} from './editors';
import { Switch } from './ui';

type Selection = { kind: 'leveling' | 'headroom' | 'src' | 'output' } | { kind: 'filter'; id: string } | null;

const FILTER_ICON: Record<FilterType, ReactNode> = {
  crossfeed: <Headphones size={18} />,
  peq: <AudioWaveform size={18} />,
  procedural: <SlidersHorizontal size={18} />,
  headphone: <Headphones size={18} />,
  convolution: <Waves size={18} />,
};

function filterSummary(f: DspFilter): string {
  switch (f.type) {
    case 'crossfeed':
      return f.preset === 'custom' ? `${f.fcut}Hz · ${f.feed}dB` : { default: '기본', cmoy: 'Chu Moy', jmeier: 'Jan Meier' }[f.preset];
    case 'peq':
      return `밴드 ${f.bands.filter((b) => b.on).length}개`;
    case 'headphone':
      return f.model || (f.bands.length ? `밴드 ${f.bands.length}개` : '프리셋 없음');
    case 'procedural':
      return `단계 ${f.steps.length}개`;
    case 'convolution':
      return f.irName ?? '파일 없음';
  }
}

/** 시작 화면의 그림: 소리의 산맥과 그 그림자. 룬의 MUSE 첫 화면에서 빌린 생각. */
function Ridge() {
  const rows = 22;
  const cols = 64;
  const lines: string[] = [];
  const shadow: string[] = [];
  const h = (u: number, v: number) => {
    const ridge = Math.exp(-Math.pow((u - 0.5) / 0.16, 2)) * (0.75 + 0.25 * Math.sin(u * 37 + v * 3));
    const side = Math.exp(-Math.pow((u - 0.3) / 0.12, 2)) * 0.35 + Math.exp(-Math.pow((u - 0.68) / 0.1, 2)) * 0.45;
    const noise = 0.08 * Math.sin(u * 91 + v * 13) * Math.sin(u * 23 - v * 7);
    return Math.max(0, (ridge + side + noise) * Math.exp(-Math.pow((v - 0.55) / 0.35, 2)));
  };
  for (let r = 0; r < rows; r++) {
    const v = r / (rows - 1);
    const pts: string[] = [];
    const sh: string[] = [];
    for (let c = 0; c < cols; c++) {
      const u = c / (cols - 1);
      const px = 30 + u * 540 + (v - 0.5) * 40;
      const base = 120 + v * 26;
      const y = base - h(u, v) * 95;
      pts.push(`${px.toFixed(1)},${y.toFixed(1)}`);
      sh.push(`${px.toFixed(1)},${(base + 6 + h(u, v) * 70 * (1 - v * 0.4)).toFixed(1)}`);
    }
    lines.push(pts.join(' '));
    shadow.push(sh.join(' '));
  }
  return (
    <svg className="ridge" viewBox="0 0 600 260" role="img" aria-label="소리의 산맥">
      {shadow.map((p, i) => (
        <polyline key={`s${i}`} points={p} className="ridge-shadow" />
      ))}
      {lines.map((p, i) => (
        <polyline key={i} points={p} className="ridge-line" />
      ))}
    </svg>
  );
}

function SignalPreview() {
  const { current, outputRate, formatOf, caps } = useQueue();
  const s = useEngineState();
  if (!current) return <p className="muted">곡을 재생하면 여기에 지금의 신호 경로가 나옵니다.</p>;
  const sig = signalPath({
    format: formatOf(current.track),
    source: current.track.source,
    transcoded: s.transcoded,
    dsp: s.dsp,
    loudness: current.track.loudness,
    caps,
    volume: s.volume,
    muted: s.muted,
    outputRate,
  });
  return (
    <div className="dsp-signal">
      <div className="dsp-signal-head">
        신호 경로: <span className={`q-${sig.quality}`}>{SIGNAL_LABEL[sig.quality]}</span>
        <span className="muted"> · {current.track.title}</span>
      </div>
      <ol className="signal-steps">
        {sig.steps.map((st, i) => (
          <li key={i} className={`signal-step q-${st.quality}`}>
            <span className="signal-node">
              <QualityGlyph quality={st.quality} size={11} />
            </span>
            <div>
              <div className="signal-title">{st.title}</div>
              <div className="signal-detail">{st.detail}</div>
            </div>
          </li>
        ))}
      </ol>
    </div>
  );
}

function PresetMenu() {
  const { presets, preset, savePreset, loadPreset, deletePreset, reset } = useDsp();
  const [open, setOpen] = useState(false);
  const [name, setName] = useState('');
  const ref = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (!open) return;
    const down = (e: PointerEvent) => !ref.current?.contains(e.target as Node) && setOpen(false);
    document.addEventListener('pointerdown', down, true);
    return () => document.removeEventListener('pointerdown', down, true);
  }, [open]);
  return (
    <div className="preset" ref={ref}>
      <span className="preset-label">프리셋</span>
      <button type="button" className="preset-btn" onClick={() => setOpen((o) => !o)} aria-expanded={open}>
        {preset ? `${preset.name}${preset.modified ? ' (수정됨)' : ''}` : '프리셋 없음'}
        <ChevronDown size={16} />
      </button>
      {open && (
        <div className="preset-pop">
          {presets.length === 0 && <p className="muted">저장한 프리셋이 없습니다.</p>}
          {presets.map((p) => (
            <div key={p.name} className="preset-row">
              <button
                type="button"
                className={`preset-item${preset?.name === p.name ? ' is-active' : ''}`}
                onClick={() => {
                  loadPreset(p.name);
                  setOpen(false);
                }}
              >
                {p.name}
              </button>
              <button type="button" className="icon-btn" aria-label={`${p.name} 지우기`} onClick={() => deletePreset(p.name)}>
                <Trash2 size={14} />
              </button>
            </div>
          ))}
          <form
            className="preset-save"
            onSubmit={(e) => {
              e.preventDefault();
              savePreset(name || preset?.name || '내 프리셋');
              setName('');
              setOpen(false);
            }}
          >
            <input
              className="textfield"
              value={name}
              placeholder={preset?.name ?? '새 프리셋 이름'}
              onChange={(e) => setName(e.target.value)}
              aria-label="프리셋 이름"
            />
            <button type="submit" className="btn btn-sm btn-primary">
              <Save size={14} /> 저장
            </button>
          </form>
          <button
            type="button"
            className="text-btn"
            onClick={() => {
              reset();
              setOpen(false);
            }}
          >
            <RotateCcw size={13} /> 처음 상태로
          </button>
        </div>
      )}
    </div>
  );
}

function Row({
  title,
  status,
  selected,
  onSelect,
  toggle,
  icon,
  dim,
}: {
  title: string;
  status: ReactNode;
  selected: boolean;
  onSelect: () => void;
  toggle?: ReactNode;
  icon?: ReactNode;
  dim?: boolean;
}) {
  return (
    <div
      className={`muse-row${selected ? ' is-selected' : ''}${dim ? ' is-dim' : ''}`}
      onClick={onSelect}
      role="button"
      tabIndex={0}
      onKeyDown={(e) => e.key === 'Enter' && onSelect()}
    >
      {icon && <span className="muse-icon">{icon}</span>}
      <div className="muse-row-text">
        <div className="muse-row-title">{title}</div>
        <div className="muse-row-status">{toggle ?? status}</div>
      </div>
      {toggle && <span className="muse-row-sub">{status}</span>}
      <span className="muse-chevron">
        <ChevronRight size={16} />
      </span>
    </div>
  );
}

export function DspPage() {
  const { config, update } = useDsp();
  const out = useZoneOutput();
  const [sel, setSel] = useState<Selection>(null);
  const [adding, setAdding] = useState(false);
  const [drag, setDrag] = useState<number | null>(null);
  const [over, setOver] = useState<number | null>(null);

  const selected = sel?.kind === 'filter' ? config.filters.find((f) => f.id === sel.id) : undefined;
  const setFilter = (f: DspFilter) => update((c) => void (c.filters = c.filters.map((x) => (x.id === f.id ? f : x))));
  const boost = useMemo(() => filterBoost(config), [config]);
  const headroomShort = config.headroom.enabled && boost > 0 && -config.headroom.gain < boost;

  const lv = config.leveling;
  const levelStatus = lv.mode === 'off' ? '꺼짐' : `${{ auto: '자동', track: '트랙', album: '앨범' }[lv.mode]} · ${lv.target} LUFS`;

  const editor = (() => {
    if (!sel) return null;
    switch (sel.kind) {
      case 'leveling':
        return { title: '볼륨 레벨링', body: <LevelingEditor /> };
      case 'headroom':
        return { title: '헤드룸 관리', body: <HeadroomEditor /> };
      case 'src':
        return { title: '샘플레이트 변환', body: <SrcEditor /> };
      case 'output':
        return { title: '출력 비트 깊이', body: <OutputEditor /> };
      case 'filter': {
        if (!selected) return null;
        const body =
          selected.type === 'crossfeed' ? (
            <CrossfeedEditor filter={selected} set={setFilter} />
          ) : selected.type === 'peq' || selected.type === 'headphone' ? (
            <EqEditor filter={selected} set={(f) => setFilter(f)} />
          ) : selected.type === 'procedural' ? (
            <ProceduralEditor filter={selected} set={setFilter} />
          ) : (
            <ConvolutionEditor filter={selected} set={setFilter} />
          );
        return { title: FILTER_LABEL[selected.type], body, filter: selected };
      }
    }
  })();

  return (
    <div className={`muse${sel ? ' has-selection' : ''}`}>
      <header className="muse-head">
        <div className="muse-brand">
          <span className="muse-logo">소리결</span>
          <span className="muse-sub">정밀 오디오 제어</span>
        </div>
        <PresetMenu />
        <div className="muse-master">
          <span>DSP 엔진</span>
          <Switch label="DSP 엔진" on={config.enabled} onChange={(v) => update((c) => void (c.enabled = v))} />
        </div>
      </header>

      <div className="muse-body">
        <aside className="muse-list" aria-label="신호 흐름">
          <div className="muse-zone">
            {out.kind === 'native' ? <Speaker size={20} /> : <Monitor size={20} />}
            <div>
              <div className="muse-zone-name">{out.name}</div>
              <div className="muse-zone-sub">
                {out.device ? `${out.device} · ` : ''}출력 {out.rate ? `${khz(out.rate)}kHz` : '확인 전'}
              </div>
            </div>
          </div>

          <Row title="볼륨 레벨링" status={levelStatus} selected={sel?.kind === 'leveling'} onSelect={() => setSel({ kind: 'leveling' })} />
          <Row
            title="헤드룸 관리"
            status={config.headroom.enabled ? `${fmtDb(config.headroom.gain)}${headroomShort ? ' · 부족' : ''}` : '꺼짐'}
            toggle={
              <Switch label="헤드룸 관리" on={config.headroom.enabled} onChange={(v) => update((c) => void (c.headroom.enabled = v))} />
            }
            selected={sel?.kind === 'headroom'}
            onSelect={() => setSel({ kind: 'headroom' })}
            dim={!config.enabled}
          />
          <Row
            title="샘플레이트 변환"
            status={config.src.enabled ? SRC_MODE_LABEL[config.src.mode] : '꺼짐'}
            toggle={<Switch label="샘플레이트 변환" on={config.src.enabled} onChange={(v) => update((c) => void (c.src.enabled = v))} />}
            selected={sel?.kind === 'src'}
            onSelect={() => setSel({ kind: 'src' })}
            dim={!config.enabled}
          />
          <Row
            title="출력 비트 깊이"
            status={`64비트 → ${config.output.bits}비트${config.output.dither === 'tpdf' ? ' · TPDF' : ''}`}
            selected={sel?.kind === 'output'}
            onSelect={() => setSel({ kind: 'output' })}
            dim={!config.enabled}
          />

          <div className="muse-filters-head">
            <h3>필터</h3>
            <label className="muse-bypass">
              모든 필터 우회
              <Switch label="모든 필터 우회" on={config.bypass} onChange={(v) => update((c) => void (c.bypass = v))} showText={false} />
            </label>
          </div>

          <ol className={`muse-filters${config.bypass || !config.enabled ? ' is-bypassed' : ''}`}>
            {config.filters.map((f, i) => (
              <li
                key={f.id}
                className={`muse-filter${drag === i ? ' is-dragging' : ''}${over === i && drag !== i ? ' is-over' : ''}`}
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
                  if (drag != null && drag !== i)
                    update((c) => {
                      const [m] = c.filters.splice(drag, 1);
                      c.filters.splice(i, 0, m!);
                    });
                  setDrag(null);
                  setOver(null);
                }}
                onDragEnd={() => {
                  setDrag(null);
                  setOver(null);
                }}
              >
                <span className="muse-grip" aria-hidden>
                  <GripVertical size={15} />
                </span>
                <Row
                  title={FILTER_LABEL[f.type]}
                  status={filterSummary(f)}
                  toggle={<Switch label={FILTER_LABEL[f.type]} on={f.enabled} onChange={(v) => setFilter({ ...f, enabled: v })} />}
                  selected={sel?.kind === 'filter' && sel.id === f.id}
                  onSelect={() => setSel({ kind: 'filter', id: f.id })}
                  icon={FILTER_ICON[f.type]}
                />
              </li>
            ))}
          </ol>

          <div className="muse-add">
            <button type="button" className="btn btn-sm" onClick={() => setAdding((v) => !v)} disabled={config.filters.length >= 16}>
              <Plus size={15} /> 필터 추가
            </button>
            {adding && (
              <div className="muse-add-menu">
                {(Object.keys(FILTER_LABEL) as FilterType[]).map((t) => (
                  <button
                    key={t}
                    type="button"
                    className="muse-add-item"
                    onClick={() => {
                      const f = { ...newFilter(t), enabled: true };
                      update((c) => void c.filters.push(f));
                      setSel({ kind: 'filter', id: f.id });
                      setAdding(false);
                    }}
                  >
                    <span className="muse-icon">{FILTER_ICON[t]}</span>
                    <span>
                      <span className="choice-title">{FILTER_LABEL[t]}</span>
                      <span className="choice-hint">{FILTER_BLURB[t]}</span>
                    </span>
                  </button>
                ))}
              </div>
            )}
          </div>
        </aside>

        <section className="muse-panel">
          {editor ? (
            <>
              <div className="muse-panel-head">
                <button type="button" className="icon-btn muse-back" onClick={() => setSel(null)} aria-label="목록으로">
                  <ChevronLeft size={20} />
                </button>
                {editor.filter && <span className="muse-icon muse-icon-lg">{FILTER_ICON[editor.filter.type]}</span>}
                <h2>{editor.title}</h2>
                {editor.filter && (
                  <div className="muse-panel-actions">
                    <Switch
                      label={editor.title}
                      on={editor.filter.enabled}
                      onChange={(v) => setFilter({ ...editor.filter!, enabled: v })}
                    />
                    <button
                      type="button"
                      className="text-btn"
                      onClick={() => {
                        const id = editor.filter!.id;
                        update((c) => void (c.filters = c.filters.filter((x) => x.id !== id)));
                        setSel(null);
                      }}
                    >
                      <X size={14} /> 필터 지우기
                    </button>
                  </div>
                )}
              </div>
              {!config.enabled && sel?.kind !== 'leveling' && (
                <div className="dsp-note is-warn">DSP 엔진이 꺼져 있습니다. 위의 스위치를 켜야 이 설정이 소리에 반영됩니다.</div>
              )}
              {editor.filter && config.bypass && <div className="dsp-note is-warn">모든 필터 우회가 켜져 있습니다.</div>}
              <div className="muse-editor">{editor.body}</div>
            </>
          ) : (
            <div className="muse-intro">
              <h2>정밀 오디오 제어</h2>
              <p>
                소리결은 EQ, 크로스피드, 샘플레이트 변환, 컨볼루션 같은 도구로 오디오 신호를 계산해 바꿉니다. 모든 계산은 소리 엔진이
                64비트 부동소수점으로 하고, 결과는 손실 없이 {out.name}에 갑니다.
              </p>
              <Ridge />
              <p>
                완벽한 시스템도, 완벽한 방도, 완벽한 음원도 없습니다. 각자가 가진 것이 제 실력을 내도록 돕는 도구들입니다. 아무것도 켜지
                않으면 원본 파일이 바이트 그대로 전송됩니다.
              </p>
              {headroomShort && (
                <div className="dsp-note is-warn">
                  켜진 필터가 최대 {fmtDb(boost)} 올리는데 헤드룸은 {fmtDb(config.headroom.gain)}입니다. 클리핑이 날 수 있습니다.
                </div>
              )}
              <SignalPreview />
              <p className="muted">
                <Combine size={13} /> 필터는 끌어서 순서를 바꿀 수 있고, 위에서 아래로 차례로 적용됩니다. 설정은 이 기기에 저장됩니다.
              </p>
            </div>
          )}
        </section>
      </div>
    </div>
  );
}
