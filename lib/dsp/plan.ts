/**
 * 요청 하나 → ffmpeg 필터 그래프 + 신호 경로의 단계들.
 *
 * 같은 함수가 서버에서는 실제로 걸 그래프를, 브라우저에서는 화면에 보일 단계를 만든다. 그래서
 * "보이는 것"과 "하는 것"이 어긋날 수 없다.
 *
 * 체인의 모양(룬과 같다):
 *   원본 → 64비트 부동소수점 → 볼륨 레벨링 → 헤드룸 → [사용자가 순서를 정한 필터들]
 *        → 샘플레이트 변환 → 비트 깊이 변환(디더) → FLAC으로 무손실 전송
 */
import type { AudioFormat } from '../library/types';
import { khz } from '../quality';
import { BAND_LABEL } from './biquad';
import { CROSSFEED_PRESETS } from './defaults';
import { levelingGain, type LevelingResult } from './leveling';
import type { Band, Channel, DspRequest, Loudness, Step, WireFilter } from './types';

export type StageQuality = 'lossless' | 'enhanced' | 'high' | 'low';

export interface Stage {
  title: string;
  detail: string;
  quality: StageQuality;
}

export interface Caps {
  soxr: boolean;
  bs2b: boolean;
}

export const FULL_CAPS: Caps = { soxr: true, bs2b: true };

export type SourceInfo = Pick<AudioFormat, 'codec' | 'lossless' | 'sampleRate' | 'bitDepth' | 'channels'>;

export interface Plan {
  stages: Stage[];
  /** ffmpeg -filter_complex 값. 입력 [0:a:0] → 출력 [out] */
  graph: string;
  /** 추가 입력(임펄스 응답). 순서대로 ffmpeg 입력 1, 2, … */
  irs: { id: string }[];
  /** 내보내는 샘플레이트. 모르면(라이브 원본 그대로) undefined */
  outRate?: number;
  outBits: 16 | 24;
  clip: boolean;
  level?: LevelingResult;
}

/** 필터 그래프에 넣는 숫자. 지수 표기나 "-0"이 나오지 않게. */
function fx(n: number, digits = 4): string {
  const s = n.toFixed(digits);
  return /^-0\.?0*$/.test(s) ? s.slice(1) : s;
}

export function fmtDb(db: number): string {
  return `${db > 0 ? '+' : db < 0 ? '−' : ''}${Math.abs(db).toFixed(1)}dB`;
}

function chName(ch: Channel) {
  return ch === 'L' ? '왼쪽' : ch === 'R' ? '오른쪽' : '양쪽';
}

function bandFilter(b: Band, fs: number | undefined, ch: Channel = 'LR'): string {
  // 나이퀴스트를 넘는 주파수는 ffmpeg가 거부한다. 원본 샘플레이트를 알면 그 안으로 자른다.
  const f = fs ? Math.min(b.freq, fs / 2 - 10) : Math.min(b.freq, 21000);
  const c = ch === 'L' ? ':c=FL' : ch === 'R' ? ':c=FR' : '';
  const common = `f=${fx(f, 2)}:t=q:w=${fx(b.q, 4)}`;
  switch (b.type) {
    case 'peak':
      return `equalizer=${common}:g=${fx(b.gain, 3)}:precision=f64${c}`;
    case 'lowshelf':
      return `lowshelf=${common}:g=${fx(b.gain, 3)}:precision=f64${c}`;
    case 'highshelf':
      return `highshelf=${common}:g=${fx(b.gain, 3)}:precision=f64${c}`;
    case 'lowpass':
      return `lowpass=${common}:p=2:precision=f64${c}`;
    case 'highpass':
      return `highpass=${common}:p=2:precision=f64${c}`;
    case 'bandpass':
      return `bandpass=${common}:precision=f64${c}`;
    case 'notch':
      return `bandreject=${common}:precision=f64${c}`;
  }
}

const lin = (db: number) => Math.pow(10, db / 20);

/** pan 필터 한 줄. 계수는 64비트로 계산된다. */
function pan(ll: number, lr: number, rl: number, rr: number): string {
  const term = (g: number, c: string, first: boolean) => {
    if (g === 0) return '';
    const sign = g < 0 ? '-' : first ? '' : '+';
    return `${sign}${fx(Math.abs(g), 9)}*${c}`;
  };
  const expr = (a: number, b: number) => {
    const s = term(a, 'c0', true) + term(b, 'c1', a === 0);
    return s || '0*c0';
  };
  return `pan=stereo|c0=${expr(ll, lr)}|c1=${expr(rl, rr)}`;
}

function stepFilter(s: Step, fs: number | undefined): string | null {
  const gains = (ch: Channel, g: number) => [ch === 'R' ? 1 : g, ch === 'L' ? 1 : g] as const;
  switch (s.op) {
    case 'volume': {
      if (s.ch === 'LR') return `volume=${fx(s.gain, 3)}dB:precision=double`;
      const [l, r] = gains(s.ch, lin(s.gain));
      return pan(l, 0, 0, r);
    }
    case 'delay': {
      if (s.ms <= 0) return null;
      const l = s.ch === 'R' ? 0 : s.ms;
      const r = s.ch === 'L' ? 0 : s.ms;
      return `adelay=delays=${fx(l, 3)}|${fx(r, 3)}`;
    }
    case 'invert': {
      const [l, r] = gains(s.ch, -1);
      return pan(l, 0, 0, r);
    }
    case 'mute': {
      const [l, r] = gains(s.ch, 0);
      return pan(l, 0, 0, r);
    }
    case 'mix':
      return pan(s.ll, s.lr, s.rl, s.rr);
    case 'eq':
      return s.band.on ? bandFilter(s.band, fs, s.ch) : null;
  }
}

export function stepLabel(s: Step): string {
  switch (s.op) {
    case 'volume':
      return `볼륨 ${chName(s.ch)} ${fmtDb(s.gain)}`;
    case 'delay':
      return `지연 ${chName(s.ch)} ${s.ms.toFixed(2)}ms`;
    case 'invert':
      return `위상 반전 ${chName(s.ch)}`;
    case 'mute':
      return `음소거 ${chName(s.ch)}`;
    case 'mix':
      return `믹스 L=${fx(s.ll, 2)}L${s.lr >= 0 ? '+' : ''}${fx(s.lr, 2)}R, R=${fx(s.rl, 2)}L${s.rr >= 0 ? '+' : ''}${fx(s.rr, 2)}R`;
    case 'eq':
      return `${BAND_LABEL[s.band.type]} ${chName(s.ch)} ${Math.round(s.band.freq)}Hz`;
  }
}

function crossfeedParams(f: Extract<WireFilter, { t: 'crossfeed' }>) {
  if (f.p === 'custom') return { fcut: f.fc ?? 700, feed: f.fd ?? 4.5, label: '사용자 지정' };
  return CROSSFEED_PRESETS[f.p];
}

export function planDsp(
  req: DspRequest,
  src: SourceInfo,
  loudness: Loudness | undefined,
  opts: { caps?: Caps; irMono?: (id: string) => boolean } = {},
): Plan {
  const caps = opts.caps ?? FULL_CAPS;
  const fs = src.sampleRate;
  const stages: Stage[] = [];
  const parts: string[] = [];
  const irs: { id: string }[] = [];
  let label = '0:a:0';
  let cur: string[] = ['aformat=sample_fmts=dblp'];
  let seg = 0;
  const flush = (out: string) => {
    parts.push(`[${label}]${cur.length ? cur.join(',') : 'anull'}[${out}]`);
    label = out;
    cur = [];
  };

  // 1. 64비트 부동소수점으로
  const dsd = src.codec === 'DSD';
  stages.push({
    title: '비트 깊이 변환',
    detail: dsd
      ? 'DSD → PCM 64비트 부동소수점'
      : src.lossless
        ? `${src.bitDepth ? `${src.bitDepth}비트` : src.codec} → 64비트 부동소수점`
        : `${src.codec} 디코딩 → 64비트 부동소수점`,
    quality: dsd ? 'high' : 'lossless',
  });

  // 채널: 모노는 양쪽에 그대로, 3채널 이상은 스테레오로 다운믹스
  if (src.channels === 1) {
    cur.push('pan=stereo|c0=c0|c1=c0');
  } else if (src.channels && src.channels > 2) {
    cur.push('aformat=sample_fmts=dblp:channel_layouts=stereo');
    stages.push({ title: '다운믹스', detail: `${src.channels}채널 → 스테레오`, quality: 'high' });
  }

  // 2. 볼륨 레벨링
  let level: LevelingResult | undefined;
  if (req.lv) {
    level = levelingGain(loudness, req.lv.m, req.lv.t, req.lv.u);
    if (level.gain !== 0) cur.push(`volume=${fx(level.gain, 2)}dB:precision=double`);
    stages.push({
      title: '볼륨 레벨링',
      detail:
        level.basis === 'unknown'
          ? `${fmtDb(level.gain)} · 라우드니스 정보 없음`
          : `${fmtDb(level.gain)} · ${level.basis === 'album' ? '앨범' : '트랙'} ${level.lufs!.toFixed(1)} LUFS → ${req.lv.t} LUFS${level.limited ? ' · 트루 피크에서 멈춤' : ''}`,
      quality: 'enhanced',
    });
  }

  // 3. 헤드룸 — 체인 맨 앞에 고정
  if (req.hr != null && req.hr !== 0) {
    cur.push(`volume=${fx(req.hr, 2)}dB:precision=double`);
    stages.push({ title: '헤드룸 조정', detail: fmtDb(req.hr), quality: 'enhanced' });
  }

  // 4. 필터들, 사용자가 정한 순서대로
  for (const f of req.f) {
    switch (f.t) {
      case 'crossfeed': {
        const p = crossfeedParams(f);
        if (caps.bs2b) {
          cur.push(f.p === 'custom' ? `bs2b=fcut=${Math.round(p.fcut)}:feed=${Math.round(p.feed * 10)}` : `bs2b=profile=${f.p}`);
        } else {
          // bs2b가 없는 ffmpeg: 내장 crossfeed로 비슷하게
          cur.push(`crossfeed=strength=${fx(Math.min(1, p.feed / 15), 3)}:range=${fx(Math.min(1, p.fcut / 2000), 3)}`);
        }
        stages.push({
          title: '크로스피드',
          detail: `${p.label} · ${Math.round(p.fcut)}Hz, ${p.feed.toFixed(1)}dB${caps.bs2b ? '' : ' (근사)'}`,
          quality: 'enhanced',
        });
        break;
      }
      case 'peq':
      case 'headphone': {
        if (f.pre) cur.push(`volume=${fx(f.pre, 2)}dB:precision=double`);
        for (const b of f.b) cur.push(bandFilter(b, fs));
        const what = `밴드 ${f.b.length}개${f.pre ? ` · 프리앰프 ${fmtDb(f.pre)}` : ''}`;
        stages.push({
          title: f.t === 'peq' ? '파라메트릭 EQ' : '헤드폰 EQ',
          detail: f.t === 'headphone' && f.m ? `${f.m} · ${what}` : what,
          quality: 'enhanced',
        });
        break;
      }
      case 'procedural': {
        for (const s of f.s) {
          const x = stepFilter(s, fs);
          if (x) cur.push(x);
        }
        stages.push({ title: '프로시저럴 EQ', detail: f.s.map(stepLabel).join(' · ') || '단계 없음', quality: 'enhanced' });
        break;
      }
      case 'convolution': {
        const k = irs.length + 1;
        irs.push({ id: f.ir });
        flush(`s${seg++}`);
        const ir = `ir${k}`;
        parts.push(
          `[${k}:a:0]aformat=sample_fmts=dblp${fs ? `,aresample=resampler=${caps.soxr ? 'soxr:precision=28' : 'swr'}:osr=${fs}` : ''}[${ir}]`,
        );
        const mono = opts.irMono?.(f.ir) ?? false;
        parts.push(`[${label}][${ir}]afir=gtype=${f.n ? 'peak' : 'none'}:irfmt=${mono ? 'mono' : 'input'}:precision=double[c${k}]`);
        label = `c${k}`;
        cur = ['aformat=sample_fmts=dblp'];
        if (f.g) cur.push(`volume=${fx(f.g, 2)}dB:precision=double`);
        stages.push({ title: '컨볼루션', detail: `${f.name ?? '임펄스 응답'}${f.g ? ` · ${fmtDb(f.g)}` : ''}`, quality: 'enhanced' });
        break;
      }
    }
  }

  // 5. 샘플레이트 변환. DSD는 브라우저가 받을 수 있는 PCM 샘플레이트로 반드시 내린다.
  let outRate = fs;
  const target = req.src?.r ?? (dsd ? 176400 : undefined);
  if (target && target !== fs) {
    const precise = (req.src?.f ?? 'precise') === 'precise';
    const p = req.src?.p ?? 28;
    cur.push(
      caps.soxr
        ? `aresample=resampler=soxr:precision=${p}:osr=${target}${precise ? ':cheby=1' : ':cutoff=0.8'}`
        : `aresample=osr=${target}:filter_size=256:phase_shift=14:linear_interp=0:cutoff=${precise ? '0.97' : '0.88'}`,
    );
    outRate = target;
    stages.push({
      title: '샘플레이트 변환',
      detail: `${fs ? `${khz(fs)}kHz` : '원본'} → ${khz(target)}kHz · ${caps.soxr ? `SoX ${p}비트` : 'swr'}, ${precise ? '정밀' : '부드럽게'} · 선형 위상${!req.src && dsd ? ' (호환성)' : ''}`,
      quality: 'high',
    });
  }

  // 클리핑 감지: 마지막 양자화 바로 앞에서 0dBFS를 넘는 프레임만 기록한다.
  const clip = !!req.ci;
  if (clip) {
    cur.push(
      'astats=metadata=1:reset=1:measure_perchannel=none:measure_overall=Peak_level',
      'ametadata=mode=print:key=lavfi.astats.Overall.Peak_level:value=0:function=greater',
    );
  }

  // 6. 64비트 → 전송 비트 깊이
  const bits = req.out.b;
  const dither = req.out.d === 'tpdf' ? 'triangular' : 'none';
  cur.push(bits === 24 ? `aresample=osf=s32:output_sample_bits=24:dither_method=${dither}` : `aresample=osf=s16:dither_method=${dither}`);
  stages.push({
    title: '비트 깊이 변환',
    detail: `64비트 부동소수점 → ${bits}비트${req.out.d === 'tpdf' ? ' · TPDF 디더' : ''}`,
    quality: 'high',
  });

  flush('out');
  return { stages, graph: parts.join(';'), irs, outRate, outBits: bits, clip, level };
}
