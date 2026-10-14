/**
 * 설정 → 곡 하나에 대한 요청, 그리고 그 반대.
 *
 * 서버는 클라이언트가 보낸 요청을 믿지 않는다. 모든 숫자를 범위 안으로 자르고, 이름은 정해진
 * 목록에서만 고른다. ffmpeg 필터 그래프에 들어가는 글자는 전부 여기를 지나간 숫자와 낱말뿐이다.
 */
import { STANDARD_RATES } from './defaults';
import { srcTarget } from './src';
import type { Band, BandType, Channel, DspConfig, DspFilter, DspRequest, Step, WireFilter } from './types';

export interface RequestContext {
  /** 원본 샘플레이트(모르면 undefined) */
  sourceRate?: number;
  /** 출력 장치 샘플레이트(아직 모르면 null) */
  deviceRate: number | null;
  /** 앞뒤 곡이 같은 앨범인지 — 레벨링 "자동"이 앨범 기준을 쓸지 정한다 */
  sameAlbumNeighbors: boolean;
}

function effectiveBands(bands: Band[]) {
  return bands.filter(
    (b) => b.on && (b.type === 'lowpass' || b.type === 'highpass' || b.type === 'bandpass' || b.type === 'notch' || b.gain !== 0),
  );
}

function toWire(f: DspFilter): WireFilter | null {
  switch (f.type) {
    case 'crossfeed':
      return f.preset === 'custom' ? { t: 'crossfeed', p: 'custom', fc: f.fcut, fd: f.feed } : { t: 'crossfeed', p: f.preset };
    case 'peq': {
      const b = effectiveBands(f.bands);
      return b.length || f.preamp ? { t: 'peq', pre: f.preamp, b } : null;
    }
    case 'headphone': {
      const b = effectiveBands(f.bands);
      return b.length || f.preamp ? { t: 'headphone', pre: f.preamp, b, m: f.model || undefined } : null;
    }
    case 'procedural':
      return f.steps.length ? { t: 'procedural', s: f.steps } : null;
    case 'convolution':
      return f.ir ? { t: 'convolution', ir: f.ir, g: f.gain, n: f.normalize, name: f.irName } : null;
  }
}

/** 이 곡에 무언가를 해야 하면 요청을, 아무것도 안 해도 되면 null(원본 그대로)을. */
export function buildRequest(cfg: DspConfig, ctx: RequestContext): DspRequest | null {
  const req: DspRequest = { v: 1, out: { b: cfg.output.bits, d: cfg.output.dither }, f: [] };
  let active = false;
  if (cfg.leveling.mode !== 'off') {
    const m = cfg.leveling.mode === 'auto' ? (ctx.sameAlbumNeighbors ? 'album' : 'track') : cfg.leveling.mode;
    req.lv = { m, t: cfg.leveling.target, u: cfg.leveling.unknown };
    active = true;
  }
  if (cfg.enabled) {
    if (cfg.headroom.enabled && cfg.headroom.gain < 0) {
      req.hr = cfg.headroom.gain;
      active = true;
    }
    if (cfg.headroom.enabled && cfg.headroom.clipIndicator) req.ci = 1;
    const r = srcTarget(cfg.src, ctx.sourceRate, ctx.deviceRate);
    if (r) {
      req.src = { r, f: cfg.src.filter, p: cfg.src.precision };
      active = true;
    }
    if (!cfg.bypass) {
      for (const f of cfg.filters) {
        if (!f.enabled) continue;
        const w = toWire(f);
        if (w) {
          req.f.push(w);
          active = true;
        }
      }
    }
  }
  return active ? req : null;
}

// ---------------------------------------------------------------- 주소에 담기

function toBase64Url(s: string): string {
  const bytes = new TextEncoder().encode(s);
  let bin = '';
  for (const b of bytes) bin += String.fromCharCode(b);
  return btoa(bin).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '');
}

function fromBase64Url(s: string): string {
  const bin = atob(s.replace(/-/g, '+').replace(/_/g, '/'));
  const bytes = Uint8Array.from(bin, (c) => c.charCodeAt(0));
  return new TextDecoder().decode(bytes);
}

export function encodeRequest(req: DspRequest): string {
  return toBase64Url(JSON.stringify(req));
}

// ---------------------------------------------------------------- 검사

const num = (v: unknown, lo: number, hi: number, fallback: number): number => {
  const n = typeof v === 'number' ? v : Number(v);
  if (!Number.isFinite(n)) return fallback;
  return Math.min(hi, Math.max(lo, n));
};

const oneOf = <T extends string>(v: unknown, list: readonly T[], fallback: T): T => (list.includes(v as T) ? (v as T) : fallback);

const BAND_TYPES: readonly BandType[] = ['peak', 'lowshelf', 'highshelf', 'lowpass', 'highpass', 'bandpass', 'notch'];
const CHANNELS: readonly Channel[] = ['L', 'R', 'LR'];

function band(v: unknown): Band | null {
  if (!v || typeof v !== 'object') return null;
  const b = v as Record<string, unknown>;
  return {
    type: oneOf(b.type, BAND_TYPES, 'peak'),
    freq: num(b.freq, 10, 40000, 1000),
    gain: num(b.gain, -30, 30, 0),
    q: num(b.q, 0.05, 30, 1),
    on: b.on !== false,
  };
}

function bands(v: unknown): Band[] {
  return Array.isArray(v) ? (v.slice(0, 32).map(band).filter(Boolean) as Band[]) : [];
}

function step(v: unknown): Step | null {
  if (!v || typeof v !== 'object') return null;
  const s = v as Record<string, unknown>;
  const ch = oneOf(s.ch, CHANNELS, 'LR');
  switch (s.op) {
    case 'volume':
      return { op: 'volume', ch, gain: num(s.gain, -60, 24, 0) };
    case 'delay':
      return { op: 'delay', ch, ms: num(s.ms, 0, 500, 0) };
    case 'invert':
      return { op: 'invert', ch };
    case 'mute':
      return { op: 'mute', ch };
    case 'mix':
      return { op: 'mix', ll: num(s.ll, -4, 4, 1), lr: num(s.lr, -4, 4, 0), rl: num(s.rl, -4, 4, 0), rr: num(s.rr, -4, 4, 1) };
    case 'eq': {
      const b = band(s.band);
      return b ? { op: 'eq', ch, band: b } : null;
    }
    default:
      return null;
  }
}

const IR_ID = /^[a-z0-9]{6,40}$/;

function wire(v: unknown): WireFilter | null {
  if (!v || typeof v !== 'object') return null;
  const f = v as Record<string, unknown>;
  switch (f.t) {
    case 'crossfeed': {
      const p = oneOf(f.p, ['default', 'cmoy', 'jmeier', 'custom'] as const, 'default');
      return p === 'custom'
        ? { t: 'crossfeed', p, fc: Math.round(num(f.fc, 300, 2000, 700)), fd: num(f.fd, 1, 15, 4.5) }
        : { t: 'crossfeed', p };
    }
    case 'peq':
      return { t: 'peq', pre: num(f.pre, -30, 12, 0), b: bands(f.b) };
    case 'headphone':
      return { t: 'headphone', pre: num(f.pre, -30, 12, 0), b: bands(f.b), m: typeof f.m === 'string' ? f.m.slice(0, 80) : undefined };
    case 'procedural':
      return { t: 'procedural', s: Array.isArray(f.s) ? (f.s.slice(0, 32).map(step).filter(Boolean) as Step[]) : [] };
    case 'convolution':
      if (typeof f.ir !== 'string' || !IR_ID.test(f.ir)) return null;
      return {
        t: 'convolution',
        ir: f.ir,
        g: num(f.g, -30, 12, 0),
        n: f.n !== false,
        name: typeof f.name === 'string' ? f.name.slice(0, 120) : undefined,
      };
    default:
      return null;
  }
}

/** 서버가 받은 문자열을 안전한 요청으로. 이상하면 null. */
export function parseRequest(s: string | null | undefined): DspRequest | null {
  if (!s || s.length > 16000) return null;
  let raw: Record<string, unknown>;
  try {
    raw = JSON.parse(fromBase64Url(s)) as Record<string, unknown>;
  } catch {
    return null;
  }
  if (!raw || typeof raw !== 'object' || raw.v !== 1) return null;
  const out = (raw.out ?? {}) as Record<string, unknown>;
  const req: DspRequest = {
    v: 1,
    out: { b: out.b === 16 ? 16 : 24, d: out.d === 'none' ? 'none' : 'tpdf' },
    f: Array.isArray(raw.f) ? (raw.f.slice(0, 16).map(wire).filter(Boolean) as WireFilter[]) : [],
  };
  if (raw.hr != null) req.hr = num(raw.hr, -30, 0, 0);
  if (raw.ci) req.ci = 1;
  if (raw.lv && typeof raw.lv === 'object') {
    const lv = raw.lv as Record<string, unknown>;
    req.lv = { m: lv.m === 'album' ? 'album' : 'track', t: num(lv.t, -30, -5, -14), u: num(lv.u, -20, 6, -5) };
  }
  if (raw.src && typeof raw.src === 'object') {
    const src = raw.src as Record<string, unknown>;
    const r = Number(src.r);
    if (STANDARD_RATES.includes(r))
      req.src = { r, f: src.f === 'smooth' ? 'smooth' : 'precise', p: [20, 28, 33].includes(Number(src.p)) ? Number(src.p) : 28 };
  }
  return req;
}
