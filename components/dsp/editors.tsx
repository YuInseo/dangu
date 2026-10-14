'use client';

import { useEffect, useMemo, useRef, useState } from 'react';
import { formatAutoEq, parseAutoEq } from '@/lib/dsp/autoeq';
import { BAND_LABEL, hasGain, maxBoost } from '@/lib/dsp/biquad';
import { CROSSFEED_PRESETS, STANDARD_RATES, defaultBand } from '@/lib/dsp/defaults';
import { levelingGain } from '@/lib/dsp/leveling';
import { fmtDb } from '@/lib/dsp/plan';
import { SRC_MODE_LABEL, srcTarget } from '@/lib/dsp/src';
import { useDsp } from '@/lib/dsp/store';
import type {
  Band,
  BandType,
  Channel,
  ConvolutionFilter,
  CrossfeedFilter,
  DspConfig,
  HeadphoneFilter,
  PeqFilter,
  ProceduralFilter,
  Step,
} from '@/lib/dsp/types';
import type { AnalysisStatus } from '@/lib/library/types';
import { useQueue } from '@/lib/player/context';
import { khz } from '@/lib/quality';
import { count } from '@/lib/time';
import { useClipIndicator } from '../signal-path';
import { useToast } from '../toast';
import { ChevronDown, ChevronUp, FileAudio, Plus, Trash2, Upload, X } from '../icons';
import { BAND_COLORS, EqGraph } from './eq-graph';
import { Field, NumberField, RangeField, Seg, Switch } from './ui';

// ---------------------------------------------------------------- 볼륨 레벨링

export function LevelingEditor() {
  const { config, update } = useDsp();
  const { current } = useQueue();
  const lv = config.leveling;
  const [status, setStatus] = useState<AnalysisStatus | null>(null);

  useEffect(() => {
    let alive = true;
    const load = () =>
      fetch('/api/analysis', { cache: 'no-store' })
        .then((r) => r.json())
        .then((s: AnalysisStatus) => alive && setStatus(s))
        .catch(() => {});
    void load();
    const t = setInterval(load, 5000);
    return () => {
      alive = false;
      clearInterval(t);
    };
  }, []);

  const loud = current?.track.loudness;
  const now = lv.mode !== 'off' ? levelingGain(loud, lv.mode === 'album' ? 'album' : 'track', lv.target, lv.unknown) : null;

  return (
    <>
      <p className="dsp-lede">
        곡마다 라우드니스를 맞춰, 앨범과 곡이 바뀌어도 소리 크기가 들쭉날쭉하지 않게 합니다. 값은 백그라운드 오디오 분석(EBU R128)에서 오고,
        분석 전에는 ReplayGain 태그를 씁니다. 올리는 게인은 트루 피크를 넘지 않게 멈추므로 레벨링 때문에 클리핑이 나지 않습니다.
      </p>
      <Field label="방식" hint={MODE_HINT[lv.mode]}>
        <Seg
          label="볼륨 레벨링 방식"
          value={lv.mode}
          options={[
            { value: 'off', label: '끔' },
            { value: 'auto', label: '자동' },
            { value: 'track', label: '트랙' },
            { value: 'album', label: '앨범' },
          ]}
          onChange={(v) => update((c) => void (c.leveling.mode = v))}
        />
      </Field>
      <Field label="목표 라우드니스" hint="업계 표준은 −14 LUFS입니다. 다이내믹이 큰 음반이 많으면 −18이나 −23으로 낮추세요.">
        <Seg
          label="목표 라우드니스"
          value={lv.target}
          options={[-14, -16, -18, -20, -23].map((v) => ({ value: v, label: `${v} LUFS` }))}
          onChange={(v) => update((c) => void (c.leveling.target = v))}
        />
      </Field>
      <Field
        label="라우드니스를 모를 때"
        hint="분석 전 곡과 라이브 아카이브에 줄 고정 게인. 레벨링은 대부분 소리를 줄이므로, 갑자기 커지지 않게 −5dB가 기본입니다."
      >
        <RangeField
          label="모를 때 게인"
          value={lv.unknown}
          min={-12}
          max={0}
          step={0.5}
          unit="dB"
          onChange={(v) => update((c) => void (c.leveling.unknown = v))}
        />
      </Field>
      <div className="dsp-note">
        {status ? (
          <>
            오디오 분석: {count(status.done)} / {count(status.total)}곡
            {status.state === 'running' ? ' · 분석 중' : status.state === 'off' ? ' · 꺼짐(설정 → 라이브러리에서 켜기)' : ''}
          </>
        ) : (
          '오디오 분석 상태를 불러오는 중…'
        )}
      </div>
      {current && now && (
        <div className="dsp-now">
          <div className="dsp-now-label">지금 곡</div>
          <div>
            {current.track.title} ·{' '}
            {now.basis === 'unknown'
              ? `라우드니스 정보 없음 → ${fmtDb(now.gain)}`
              : `${now.lufs!.toFixed(1)} LUFS(${now.basis === 'album' ? '앨범' : '트랙'}${loud?.source === 'replaygain' ? ', ReplayGain' : ''}) → ${fmtDb(now.gain)}${now.limited ? ' · 트루 피크에서 멈춤' : ''}`}
          </div>
        </div>
      )}
    </>
  );
}

const MODE_HINT: Record<DspConfig['leveling']['mode'], string> = {
  off: '원본 크기 그대로 재생합니다.',
  auto: '앞뒤 곡이 같은 앨범이면 앨범 기준, 아니면 곡 기준. 앨범의 곡 사이 크기 차이는 지키고, 섞어 들을 때는 고르게.',
  track: '모든 곡을 같은 크기로. 이어지는 앨범에서는 곡 경계에서 크기가 바뀔 수 있습니다.',
  album: '앨범 하나에 같은 게인. 마스터링 의도대로 곡 사이 크기 차이를 지킵니다.',
};

// ---------------------------------------------------------------- 헤드룸

/** 지금 켜진 필터들이 가장 크게 올리는 양. 헤드룸을 얼마나 잡아야 하는지의 근거. */
export function filterBoost(config: DspConfig): number {
  if (!config.enabled || config.bypass) return 0;
  const bands: Band[] = [];
  let pre = 0;
  let extra = 0;
  for (const f of config.filters) {
    if (!f.enabled) continue;
    if (f.type === 'peq' || f.type === 'headphone') {
      bands.push(...f.bands.filter((b) => b.on));
      pre += f.preamp;
    } else if (f.type === 'procedural') {
      for (const s of f.steps) {
        if (s.op === 'volume') extra += Math.max(0, s.gain);
        if (s.op === 'eq' && s.band.on) bands.push(s.band);
        if (s.op === 'mix')
          extra += Math.max(0, 20 * Math.log10(Math.max(Math.abs(s.ll) + Math.abs(s.lr), Math.abs(s.rl) + Math.abs(s.rr), 1e-6)));
      }
    } else if (f.type === 'convolution') {
      extra += Math.max(0, f.gain);
    }
  }
  return Math.max(0, maxBoost(bands, pre)) + extra;
}

export function HeadroomEditor() {
  const { config, update } = useDsp();
  const h = config.headroom;
  const boost = filterBoost(config);
  const suggested = boost > 0 ? -Math.ceil((boost + 0.5) * 2) / 2 : -3;
  const clip = useClipIndicator();
  return (
    <>
      <p className="dsp-lede">
        EQ로 올린 소리는 0dBFS를 넘어 깨질 수 있습니다. 헤드룸 관리는 체인 맨 앞에서 신호를 미리 낮춰 그 여유를 만듭니다. 체인 맨 앞에
        고정이며 빠지지 않습니다. 0dB면 아무것도 바꾸지 않습니다. −3dB에서 시작해 클리핑 표시등이 켜지면 더 내리세요.
      </p>
      <Field label="헤드룸 관리">
        <Switch label="헤드룸 관리" on={h.enabled} onChange={(v) => update((c) => void (c.headroom.enabled = v))} />
      </Field>
      <Field
        label="헤드룸 조정"
        hint={boost > 0 ? `지금 필터들이 최대 ${fmtDb(boost)} 올립니다. 권장: ${fmtDb(suggested)}` : '켜진 필터가 소리를 올리지 않습니다.'}
      >
        <div className="row-gap">
          <RangeField
            label="헤드룸"
            value={h.gain}
            min={-24}
            max={0}
            step={0.5}
            unit="dB"
            onChange={(v) => update((c) => void (c.headroom.gain = v))}
          />
          {boost > 0 && h.gain > suggested && (
            <button type="button" className="btn btn-sm" onClick={() => update((c) => void (c.headroom.gain = suggested))}>
              권장값으로
            </button>
          )}
        </div>
      </Field>
      <Field
        label="클리핑 표시기"
        hint="한 샘플만 넘쳐도 플레이어의 표시등이 붉게 켜집니다. 서버가 실제로 넘친 곡의 시각을 기록해, 들리는 순간에 맞춰 켭니다."
      >
        <Switch label="클리핑 표시기" on={h.clipIndicator} onChange={(v) => update((c) => void (c.headroom.clipIndicator = v))} />
      </Field>
      {clip.enabled && (
        <div className={`dsp-note${clip.count ? ' is-warn' : ''}`}>
          <span className={`clip-light${clip.lit ? ' is-on' : ''}`} />{' '}
          {clip.count ? `이 곡에서 클리핑 ${clip.count}번` : '이 곡에서 클리핑 없음'}
        </div>
      )}
    </>
  );
}

// ---------------------------------------------------------------- 샘플레이트 변환

const SRC_HINT: Record<DspConfig['src']['mode'], string> = {
  match: '서버가 SoX로 출력 장치의 샘플레이트에 맞춰 보내, 브라우저의 리샘플러를 거치지 않게 합니다. 대부분 이것이 가장 좋습니다.',
  max2: '44.1kHz 계열은 88.2·176.4kHz로, 48kHz 계열은 96·192kHz로 — 계열을 지키며 최대치까지 2배씩 올립니다.',
  compat: '원본이 최대 샘플레이트를 넘을 때만 같은 계열로 낮춥니다. 나머지는 그대로.',
  custom: '원본 샘플레이트마다 목표를 직접 정합니다.',
};

export function SrcEditor() {
  const { config, update } = useDsp();
  const { current, outputRate, formatOf } = useQueue();
  const s = config.src;
  const fmt = current ? formatOf(current.track) : null;
  const target = fmt ? srcTarget(s, fmt.sampleRate, outputRate) : null;
  return (
    <>
      <p className="dsp-lede">
        완전 동기식 폴리페이즈 리샘플러(SoX)로 64비트 부동소수점에서 바꿉니다. 브라우저는 출력 장치와 샘플레이트가 다르면 자체 리샘플러로
        바꾸는데, 그보다 정밀한 변환을 서버에서 먼저 해 두려는 것입니다.
      </p>
      <Field label="샘플레이트 변환">
        <Switch label="샘플레이트 변환" on={s.enabled} onChange={(v) => update((c) => void (c.src.enabled = v))} />
      </Field>
      <div className="choice-list" role="radiogroup" aria-label="변환 방식">
        {(Object.keys(SRC_MODE_LABEL) as DspConfig['src']['mode'][]).map((m) => (
          <button
            key={m}
            type="button"
            role="radio"
            aria-checked={s.mode === m}
            className={`choice${s.mode === m ? ' is-active' : ''}`}
            onClick={() => update((c) => void (c.src.mode = m))}
          >
            <span className="choice-dot" />
            <span>
              <span className="choice-title">{SRC_MODE_LABEL[m]}</span>
              <span className="choice-hint">{SRC_HINT[m]}</span>
            </span>
          </button>
        ))}
      </div>
      {s.mode === 'custom' && (
        <div className="src-table">
          {STANDARD_RATES.slice(0, 6).map((r) => (
            <div key={r} className="src-row">
              <span>{khz(r)}kHz →</span>
              <select
                value={s.custom[String(r)] ?? ''}
                onChange={(e) =>
                  update((c) => {
                    if (e.target.value) c.src.custom[String(r)] = Number(e.target.value);
                    else delete c.src.custom[String(r)];
                  })
                }
              >
                <option value="">그대로</option>
                {STANDARD_RATES.filter((t) => t !== r).map((t) => (
                  <option key={t} value={t}>
                    {khz(t)}kHz
                  </option>
                ))}
              </select>
            </div>
          ))}
        </div>
      )}
      {(s.mode === 'max2' || s.mode === 'compat') && (
        <Field label="최대 샘플레이트" hint="출력 장치(DAC)가 받을 수 있는 가장 높은 값.">
          <select className="select" value={s.maxRate} onChange={(e) => update((c) => void (c.src.maxRate = Number(e.target.value)))}>
            {[96000, 192000, 384000].map((r) => (
              <option key={r} value={r}>
                {khz(r)}kHz
              </option>
            ))}
          </select>
        </Field>
      )}
      <Field
        label="필터"
        hint="정밀: 통과 대역이 평탄하고 차단이 가파릅니다. 부드럽게: 일찍부터 천천히 깎아 울림(링잉)이 적습니다. 둘 다 선형 위상."
      >
        <Seg
          label="필터"
          value={s.filter}
          options={[
            { value: 'precise', label: '정밀' },
            { value: 'smooth', label: '부드럽게' },
          ]}
          onChange={(v) => update((c) => void (c.src.filter = v))}
        />
      </Field>
      <Field label="정밀도" hint="SoX의 계산 정밀도. 28비트부터 내부 계산이 64비트로 돕니다.">
        <Seg
          label="정밀도"
          value={s.precision}
          options={[
            { value: 20, label: '20비트' },
            { value: 28, label: '28비트' },
            { value: 33, label: '33비트' },
          ]}
          onChange={(v) => update((c) => void (c.src.precision = v))}
        />
      </Field>
      <div className="dsp-now">
        <div className="dsp-now-label">이 브라우저</div>
        <div>
          출력 장치 {outputRate ? `${khz(outputRate)}kHz` : '— (재생하면 확인)'}
          {fmt?.sampleRate ? ` · 지금 곡 ${khz(fmt.sampleRate)}kHz${target ? ` → ${khz(target)}kHz` : ' · 변환 없음'}` : ''}
        </div>
      </div>
    </>
  );
}

// ---------------------------------------------------------------- 출력 비트 깊이

export function OutputEditor() {
  const { config, update } = useDsp();
  const o = config.output;
  return (
    <>
      <p className="dsp-lede">
        모든 처리는 64비트 부동소수점에서 하고, 마지막에 이 비트 깊이로 줄여 FLAC으로 손실 없이 보냅니다. 줄일 때 TPDF 디더를 섞어 양자화
        잡음을 소리와 무관한 아주 작은 잡음으로 바꿉니다.
      </p>
      <Field label="비트 깊이">
        <Seg
          label="비트 깊이"
          value={o.bits}
          options={[
            { value: 24, label: '24비트' },
            { value: 16, label: '16비트' },
          ]}
          onChange={(v) => update((c) => void (c.output.bits = v))}
        />
      </Field>
      <Field label="디더" hint="끄면 그냥 반올림합니다. 24비트에서는 차이가 −140dB 아래라 들리지 않습니다.">
        <Seg
          label="디더"
          value={o.dither}
          options={[
            { value: 'tpdf', label: 'TPDF' },
            { value: 'none', label: '없음' },
          ]}
          onChange={(v) => update((c) => void (c.output.dither = v))}
        />
      </Field>
    </>
  );
}

// ---------------------------------------------------------------- 크로스피드

export function CrossfeedEditor({ filter, set }: { filter: CrossfeedFilter; set: (f: CrossfeedFilter) => void }) {
  return (
    <>
      <p className="dsp-lede">
        헤드폰은 왼쪽 소리가 오른쪽 귀에 전혀 닿지 않아 머릿속에 소리가 맺힙니다. 크로스피드는 반대쪽 채널을 낮은 주파수 위주로 조금 섞어,
        앞에 놓인 스피커로 듣는 것처럼 만듭니다. Bauer stereo-to-binaural(bs2b) 구현이며, 프리셋은 bs2b 기본값과 Chu Moy·Jan Meier의
        아날로그 회로를 본뜬 것입니다.
      </p>
      <div className="choice-list" role="radiogroup" aria-label="크로스피드 프리셋">
        {(['default', 'cmoy', 'jmeier', 'custom'] as const).map((p) => (
          <button
            key={p}
            type="button"
            role="radio"
            aria-checked={filter.preset === p}
            className={`choice${filter.preset === p ? ' is-active' : ''}`}
            onClick={() => set({ ...filter, preset: p })}
          >
            <span className="choice-dot" />
            <span>
              <span className="choice-title">{p === 'custom' ? '사용자 지정' : CROSSFEED_PRESETS[p].label}</span>
              <span className="choice-hint">
                {p === 'custom' ? '차단 주파수와 섞는 양을 직접' : `${CROSSFEED_PRESETS[p].fcut}Hz · ${CROSSFEED_PRESETS[p].feed}dB`}
              </span>
            </span>
          </button>
        ))}
      </div>
      {filter.preset === 'custom' && (
        <>
          <Field label="차단 주파수" hint="이보다 낮은 소리를 반대쪽으로 섞습니다.">
            <RangeField
              label="차단 주파수"
              value={filter.fcut}
              min={300}
              max={2000}
              step={10}
              unit="Hz"
              digits={0}
              onChange={(v) => set({ ...filter, fcut: v })}
            />
          </Field>
          <Field label="섞는 양" hint="클수록 약하게 섞입니다(감쇠량).">
            <RangeField
              label="섞는 양"
              value={filter.feed}
              min={1}
              max={15}
              step={0.5}
              unit="dB"
              onChange={(v) => set({ ...filter, feed: v })}
            />
          </Field>
        </>
      )}
    </>
  );
}

// ---------------------------------------------------------------- 파라메트릭 EQ · 헤드폰 EQ

const BAND_TYPES = Object.keys(BAND_LABEL) as BandType[];

function BandRow({
  band,
  index,
  selected,
  onSelect,
  onChange,
  onRemove,
}: {
  band: Band;
  index: number;
  selected: boolean;
  onSelect: () => void;
  onChange: (b: Band) => void;
  onRemove: () => void;
}) {
  return (
    <div className={`band-row${selected ? ' is-selected' : ''}${band.on ? '' : ' is-off'}`} onClick={onSelect}>
      <span className="band-dot" style={{ background: BAND_COLORS[index % BAND_COLORS.length] }} />
      <Switch label={`밴드 ${index + 1}`} on={band.on} onChange={(on) => onChange({ ...band, on })} showText={false} />
      <select
        className="select select-sm"
        value={band.type}
        aria-label="필터 종류"
        onChange={(e) => onChange({ ...band, type: e.target.value as BandType })}
      >
        {BAND_TYPES.map((t) => (
          <option key={t} value={t}>
            {BAND_LABEL[t]}
          </option>
        ))}
      </select>
      <NumberField
        label="주파수"
        value={band.freq}
        min={10}
        max={22000}
        step={1}
        digits={0}
        unit="Hz"
        onChange={(v) => onChange({ ...band, freq: v })}
      />
      <NumberField
        label="게인"
        value={band.gain}
        min={-30}
        max={30}
        step={0.1}
        unit="dB"
        onChange={(v) => onChange({ ...band, gain: v })}
      />
      <NumberField
        label="Q"
        value={band.q}
        min={0.05}
        max={30}
        step={0.01}
        digits={2}
        unit="Q"
        onChange={(v) => onChange({ ...band, q: v })}
      />
      <button
        type="button"
        className="icon-btn"
        aria-label="밴드 지우기"
        onClick={(e) => {
          e.stopPropagation();
          onRemove();
        }}
      >
        <X size={15} />
      </button>
      {!hasGain(band.type) && <span className="band-note">게인 없음</span>}
    </div>
  );
}

export function EqEditor({ filter, set }: { filter: PeqFilter | HeadphoneFilter; set: (f: PeqFilter | HeadphoneFilter) => void }) {
  const [sel, setSel] = useState<number | null>(null);
  const [importing, setImporting] = useState(false);
  const [text, setText] = useState('');
  const toast = useToast();
  const boost = useMemo(
    () =>
      maxBoost(
        filter.bands.filter((b) => b.on),
        filter.preamp,
      ),
    [filter.bands, filter.preamp],
  );
  const setBand = (i: number, b: Band) => set({ ...filter, bands: filter.bands.map((x, k) => (k === i ? b : x)) });
  const addBand = (b: Band) => {
    if (filter.bands.length >= 32) return;
    set({ ...filter, bands: [...filter.bands, b] });
    setSel(filter.bands.length);
  };
  const isHp = filter.type === 'headphone';

  return (
    <>
      {isHp ? (
        <p className="dsp-lede">
          헤드폰마다 다른 주파수 특성을 측정값으로 보정합니다. AutoEQ나 Equalizer APO 형식(ParametricEQ.txt)을 붙여 넣으면 밴드와 프리앰프가
          그대로 들어옵니다.
        </p>
      ) : (
        <p className="dsp-lede">점을 끌어 주파수와 게인을, 점 위에서 휠을 굴려 Q를 바꿉니다. 빈 곳을 두 번 누르면 밴드가 생깁니다.</p>
      )}

      {isHp && (
        <Field label="모델" hint="표시용 이름">
          <input
            className="textfield"
            value={(filter as HeadphoneFilter).model}
            placeholder="예: Sennheiser HD 650"
            onChange={(e) => set({ ...(filter as HeadphoneFilter), model: e.target.value })}
          />
        </Field>
      )}

      <div className="eq-wrap">
        <EqGraph bands={filter.bands} preamp={filter.preamp} selected={sel} onSelect={setSel} onChange={setBand} onAdd={addBand} />
      </div>

      <div className="eq-toolbar">
        <div className="eq-pre">
          <span>프리앰프</span>
          <NumberField
            label="프리앰프"
            value={filter.preamp}
            min={-30}
            max={12}
            step={0.1}
            unit="dB"
            onChange={(v) => set({ ...filter, preamp: v })}
          />
          {boost > 0.05 && (
            <button
              type="button"
              className="text-btn"
              onClick={() => set({ ...filter, preamp: Math.round((filter.preamp - boost) * 10) / 10 })}
            >
              최대 {fmtDb(boost)} — 프리앰프로 맞추기
            </button>
          )}
        </div>
        <div className="row-gap">
          {isHp && (
            <button type="button" className="btn btn-sm" onClick={() => setImporting((v) => !v)}>
              <FileAudio size={15} /> AutoEQ 붙여넣기
            </button>
          )}
          <button
            type="button"
            className="btn btn-sm"
            onClick={async () => {
              try {
                await navigator.clipboard.writeText(formatAutoEq(filter.preamp, filter.bands));
                toast('ParametricEQ.txt 형식으로 복사했습니다');
              } catch {
                toast('클립보드에 쓸 수 없습니다', 'error');
              }
            }}
          >
            내보내기
          </button>
          <button
            type="button"
            className="btn btn-sm"
            onClick={() => addBand(defaultBand(filter.bands.length ? Math.min(16000, filter.bands.at(-1)!.freq * 2) : 1000))}
          >
            <Plus size={15} /> 밴드
          </button>
        </div>
      </div>

      {importing && (
        <div className="import-box">
          <textarea
            className="textarea"
            rows={7}
            value={text}
            placeholder={'Preamp: -6.2 dB\nFilter 1: ON LSC Fc 105 Hz Gain 5.5 dB Q 0.70\nFilter 2: ON PK Fc 2300 Hz Gain -3.1 dB Q 1.41'}
            onChange={(e) => setText(e.target.value)}
          />
          <div className="row-gap">
            <button
              type="button"
              className="btn btn-sm btn-primary"
              onClick={() => {
                const r = parseAutoEq(text);
                if (!r.bands.length) {
                  toast('읽을 수 있는 필터가 없습니다', 'error');
                  return;
                }
                set({ ...filter, preamp: r.preamp, bands: r.bands.slice(0, 32) });
                setImporting(false);
                setText('');
                toast(`밴드 ${r.bands.length}개를 가져왔습니다${r.skipped ? ` (모르는 줄 ${r.skipped}개 건너뜀)` : ''}`);
              }}
            >
              가져오기
            </button>
            <button type="button" className="btn btn-sm" onClick={() => setImporting(false)}>
              닫기
            </button>
          </div>
        </div>
      )}

      <div className="band-list">
        {filter.bands.length === 0 && <p className="muted">밴드가 없습니다. 그래프를 두 번 누르거나 “밴드”를 누르세요.</p>}
        {filter.bands.map((b, i) => (
          <BandRow
            key={i}
            band={b}
            index={i}
            selected={sel === i}
            onSelect={() => setSel(i)}
            onChange={(nb) => setBand(i, nb)}
            onRemove={() => {
              set({ ...filter, bands: filter.bands.filter((_, k) => k !== i) });
              setSel(null);
            }}
          />
        ))}
      </div>
    </>
  );
}

// ---------------------------------------------------------------- 프로시저럴 EQ

const STEP_LABEL: Record<Step['op'], string> = {
  volume: '볼륨',
  delay: '지연',
  invert: '위상 반전',
  mute: '채널 음소거',
  mix: '믹스',
  eq: 'EQ 밴드',
};

function newStep(op: Step['op']): Step {
  switch (op) {
    case 'volume':
      return { op, ch: 'LR', gain: -1 };
    case 'delay':
      return { op, ch: 'L', ms: 0.5 };
    case 'invert':
      return { op, ch: 'L' };
    case 'mute':
      return { op, ch: 'L' };
    case 'mix':
      return { op, ll: 1, lr: 0, rl: 0, rr: 1 };
    case 'eq':
      return { op, ch: 'LR', band: defaultBand(1000) };
  }
}

const MIX_PRESETS = [
  { label: '그대로', m: { ll: 1, lr: 0, rl: 0, rr: 1 } },
  { label: '모노', m: { ll: 0.5, lr: 0.5, rl: 0.5, rr: 0.5 } },
  { label: '좌우 바꾸기', m: { ll: 0, lr: 1, rl: 1, rr: 0 } },
  { label: '사이드만(M/S)', m: { ll: 0.5, lr: -0.5, rl: -0.5, rr: 0.5 } },
];

function ChannelPick({ ch, onChange }: { ch: Channel; onChange: (c: Channel) => void }) {
  return (
    <Seg
      label="채널"
      value={ch}
      options={[
        { value: 'L', label: 'L' },
        { value: 'R', label: 'R' },
        { value: 'LR', label: 'L+R' },
      ]}
      onChange={onChange}
    />
  );
}

export function ProceduralEditor({ filter, set }: { filter: ProceduralFilter; set: (f: ProceduralFilter) => void }) {
  const [adding, setAdding] = useState(false);
  const setStep = (i: number, s: Step) => set({ ...filter, steps: filter.steps.map((x, k) => (k === i ? s : x)) });
  const move = (i: number, d: number) => {
    const j = i + d;
    if (j < 0 || j >= filter.steps.length) return;
    const steps = [...filter.steps];
    [steps[i], steps[j]] = [steps[j]!, steps[i]!];
    set({ ...filter, steps });
  };
  return (
    <>
      <p className="dsp-lede">
        채널마다 볼륨·지연·위상 반전·음소거·믹스·EQ 밴드를 순서대로 쌓습니다. 디지털 크로스오버, 직접 만드는 크로스피드, 수동 룸 보정처럼
        고급 설정에 씁니다. 위에서 아래로 차례로 적용됩니다.
      </p>
      <div className="step-list">
        {filter.steps.length === 0 && <p className="muted">단계가 없습니다.</p>}
        {filter.steps.map((s, i) => (
          <div key={i} className="step-row">
            <span className="step-no">{i + 1}</span>
            <span className="step-op">{STEP_LABEL[s.op]}</span>
            <div className="step-body">
              {s.op !== 'mix' && <ChannelPick ch={s.ch} onChange={(ch) => setStep(i, { ...s, ch } as Step)} />}
              {s.op === 'volume' && (
                <NumberField
                  label="게인"
                  value={s.gain}
                  min={-60}
                  max={24}
                  step={0.1}
                  unit="dB"
                  onChange={(gain) => setStep(i, { ...s, gain })}
                />
              )}
              {s.op === 'delay' && (
                <NumberField
                  label="지연"
                  value={s.ms}
                  min={0}
                  max={500}
                  step={0.01}
                  digits={2}
                  unit="ms"
                  onChange={(ms) => setStep(i, { ...s, ms })}
                />
              )}
              {s.op === 'mix' && (
                <div className="mix-grid">
                  <span>L′ =</span>
                  <NumberField
                    label="L에서 L"
                    value={s.ll}
                    min={-4}
                    max={4}
                    step={0.01}
                    digits={2}
                    unit="L"
                    onChange={(ll) => setStep(i, { ...s, ll })}
                  />
                  <NumberField
                    label="R에서 L"
                    value={s.lr}
                    min={-4}
                    max={4}
                    step={0.01}
                    digits={2}
                    unit="R"
                    onChange={(lr) => setStep(i, { ...s, lr })}
                  />
                  <span>R′ =</span>
                  <NumberField
                    label="L에서 R"
                    value={s.rl}
                    min={-4}
                    max={4}
                    step={0.01}
                    digits={2}
                    unit="L"
                    onChange={(rl) => setStep(i, { ...s, rl })}
                  />
                  <NumberField
                    label="R에서 R"
                    value={s.rr}
                    min={-4}
                    max={4}
                    step={0.01}
                    digits={2}
                    unit="R"
                    onChange={(rr) => setStep(i, { ...s, rr })}
                  />
                  <div className="mix-presets">
                    {MIX_PRESETS.map((p) => (
                      <button key={p.label} type="button" className="chip chip-sm" onClick={() => setStep(i, { op: 'mix', ...p.m })}>
                        {p.label}
                      </button>
                    ))}
                  </div>
                </div>
              )}
              {s.op === 'eq' && (
                <>
                  <select
                    className="select select-sm"
                    value={s.band.type}
                    aria-label="필터 종류"
                    onChange={(e) => setStep(i, { ...s, band: { ...s.band, type: e.target.value as BandType } })}
                  >
                    {BAND_TYPES.map((t) => (
                      <option key={t} value={t}>
                        {BAND_LABEL[t]}
                      </option>
                    ))}
                  </select>
                  <NumberField
                    label="주파수"
                    value={s.band.freq}
                    min={10}
                    max={22000}
                    step={1}
                    digits={0}
                    unit="Hz"
                    onChange={(freq) => setStep(i, { ...s, band: { ...s.band, freq } })}
                  />
                  <NumberField
                    label="게인"
                    value={s.band.gain}
                    min={-30}
                    max={30}
                    step={0.1}
                    unit="dB"
                    onChange={(gain) => setStep(i, { ...s, band: { ...s.band, gain } })}
                  />
                  <NumberField
                    label="Q"
                    value={s.band.q}
                    min={0.05}
                    max={30}
                    step={0.01}
                    digits={2}
                    unit="Q"
                    onChange={(q) => setStep(i, { ...s, band: { ...s.band, q } })}
                  />
                </>
              )}
            </div>
            <div className="step-actions">
              <button type="button" className="icon-btn" aria-label="위로" disabled={i === 0} onClick={() => move(i, -1)}>
                <ChevronUp size={16} />
              </button>
              <button
                type="button"
                className="icon-btn"
                aria-label="아래로"
                disabled={i === filter.steps.length - 1}
                onClick={() => move(i, 1)}
              >
                <ChevronDown size={16} />
              </button>
              <button
                type="button"
                className="icon-btn"
                aria-label="단계 지우기"
                onClick={() => set({ ...filter, steps: filter.steps.filter((_, k) => k !== i) })}
              >
                <X size={15} />
              </button>
            </div>
          </div>
        ))}
      </div>
      <div className="row-gap">
        <button type="button" className="btn btn-sm" onClick={() => setAdding((v) => !v)} disabled={filter.steps.length >= 32}>
          <Plus size={15} /> 단계 추가
        </button>
        {adding &&
          (Object.keys(STEP_LABEL) as Step['op'][]).map((op) => (
            <button
              key={op}
              type="button"
              className="chip chip-sm"
              onClick={() => {
                set({ ...filter, steps: [...filter.steps, newStep(op)] });
                setAdding(false);
              }}
            >
              {STEP_LABEL[op]}
            </button>
          ))}
      </div>
    </>
  );
}

// ---------------------------------------------------------------- 컨볼루션

interface IrInfo {
  id: string;
  name: string;
  channels: number;
  sampleRate: number;
  duration: number;
}

export function ConvolutionEditor({ filter, set }: { filter: ConvolutionFilter; set: (f: ConvolutionFilter) => void }) {
  const [irs, setIrs] = useState<IrInfo[] | null>(null);
  const [busy, setBusy] = useState(false);
  const toast = useToast();
  const input = useRef<HTMLInputElement>(null);
  const load = () =>
    fetch('/api/dsp/ir', { cache: 'no-store' })
      .then((r) => r.json())
      .then((j: { irs: IrInfo[] }) => setIrs(j.irs))
      .catch(() => setIrs([]));
  useEffect(() => {
    void load();
  }, []);

  const upload = async (file: File) => {
    setBusy(true);
    try {
      const form = new FormData();
      form.set('file', file);
      const r = await fetch('/api/dsp/ir', { method: 'POST', body: form });
      const j = (await r.json()) as { ir?: IrInfo; error?: string };
      if (!r.ok || !j.ir) throw new Error(j.error ?? '올리지 못했습니다');
      await load();
      set({ ...filter, ir: j.ir.id, irName: j.ir.name });
      toast(`${j.ir.name}을(를) 올렸습니다`);
    } catch (e) {
      toast(e instanceof Error ? e.message : '올리지 못했습니다', 'error');
    } finally {
      setBusy(false);
    }
  };

  return (
    <>
      <p className="dsp-lede">
        측정한 임펄스 응답으로 신호를 컨볼루션합니다. REW나 rePhase로 만든 룸 보정 필터, 헤드폰 보정 필터를 그대로 쓸 수 있습니다. 모노
        파일은 두 채널에 같이, 스테레오 파일은 채널별로 적용하고, 샘플레이트가 다르면 SoX로 맞춰 씁니다. WAV·FLAC·AIFF, 30초까지.
      </p>
      <div className="ir-list">
        {irs === null && <p className="muted">불러오는 중…</p>}
        {irs?.length === 0 && <p className="muted">올린 임펄스 응답이 없습니다.</p>}
        {irs?.map((ir) => (
          <div key={ir.id} className={`ir-row${filter.ir === ir.id ? ' is-active' : ''}`}>
            <button
              type="button"
              role="radio"
              aria-checked={filter.ir === ir.id}
              className="choice choice-inline"
              onClick={() => set({ ...filter, ir: ir.id, irName: ir.name })}
            >
              <span className="choice-dot" />
              <span>
                <span className="choice-title">{ir.name}</span>
                <span className="choice-hint">
                  {ir.channels === 1 ? '모노' : '스테레오'} · {khz(ir.sampleRate)}kHz · {ir.duration.toFixed(2)}초
                </span>
              </span>
            </button>
            <button
              type="button"
              className="icon-btn"
              aria-label="지우기"
              onClick={async () => {
                await fetch(`/api/dsp/ir?id=${ir.id}`, { method: 'DELETE' });
                if (filter.ir === ir.id) set({ ...filter, ir: null, irName: undefined });
                void load();
              }}
            >
              <Trash2 size={15} />
            </button>
          </div>
        ))}
      </div>
      <input
        ref={input}
        type="file"
        accept=".wav,.flac,.aif,.aiff,audio/wav,audio/flac,audio/aiff"
        hidden
        onChange={(e) => {
          const f = e.target.files?.[0];
          if (f) void upload(f);
          e.target.value = '';
        }}
      />
      <button type="button" className="btn btn-sm" onClick={() => input.current?.click()} disabled={busy}>
        <Upload size={15} /> {busy ? '올리는 중…' : '임펄스 응답 올리기'}
      </button>
      <Field label="정규화" hint="임펄스 응답의 가장 큰 이득을 0dB로 맞춰, 필터가 소리를 크게 바꾸지 않게 합니다.">
        <Switch label="정규화" on={filter.normalize} onChange={(v) => set({ ...filter, normalize: v })} />
      </Field>
      <Field label="출력 게인">
        <RangeField
          label="출력 게인"
          value={filter.gain}
          min={-24}
          max={12}
          step={0.5}
          unit="dB"
          onChange={(v) => set({ ...filter, gain: v })}
        />
      </Field>
    </>
  );
}
