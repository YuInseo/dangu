/**
 * 이 기기의 오디오 장치로 직접 트는 존. 소리 서버(러스트)가 풀고, 바꾸고, 내보낸다.
 *
 * 화면은 리모컨이다. 명령은 POST로 보내고, 상태는 서버가 SSE로 밀어 준다. 같은 와이파이의
 * 다른 기기(휴대폰)에서 열어도 같은 존을 조종한다. 다음 곡은 서버가 미리 열어 두었다가
 * 샘플 하나 틈 없이 이어 붙이고, 넘어간 순간을 `advanced`로 알려 준다.
 */
import type { DspPlan, EngineSnapshot, PlaybackEngine } from './engine';
import type { QueueItem } from './types';

type Listener = () => void;

export interface OutputInfo {
  rate: number;
  channels: number;
  format: 'f32' | 'f64' | 'i16' | 'i24' | 'i32';
  device: string;
  api: string;
  mixer: boolean;
}

export interface SourceInfo {
  rate: number;
  channels: number;
  bits: number | null;
  dsdRate: number | null;
  gainDb: number;
}

interface ServerState {
  status: 'idle' | 'loading' | 'playing' | 'paused' | 'error';
  uid: string | null;
  position: number;
  duration: number;
  volume: number;
  muted: boolean;
  error: string | null;
  output: OutputInfo | null;
  source: SourceInfo | null;
  resampling: [number, number] | null;
  underruns: number;
  seq: number;
  events: [number, { type: 'advanced' | 'ended' | 'error'; uid?: string | null; message?: string }][];
}

function toPlayItem(item: QueueItem, dsp: DspPlan | null) {
  const t = item.track;
  return {
    uid: item.uid,
    source: t.source === 'library' ? { kind: 'library', id: t.key.replace(/^lib:/, '') } : { kind: 'url', url: t.src },
    dsp: dsp?.q,
  };
}

async function post(path: string, body: unknown) {
  try {
    await fetch(path, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(body) });
  } catch {
    // 서버가 잠깐 없을 수 있다. 상태는 SSE가 다시 맞춘다.
  }
}

export class NativeEngine implements PlaybackEngine {
  readonly kind = 'native' as const;
  readonly zone = 'native';
  canTranscode = true;
  onEnded: (() => void) | null = null;
  onError: ((message: string) => void) | null = null;
  onAdvanced: ((uid: string) => void) | null = null;
  dspFor: ((item: QueueItem) => DspPlan | null) | null = null;

  /** 서버가 알려 준 장치와 원본 정보. 신호 경로가 쓴다. */
  output: OutputInfo | null = null;
  source: SourceInfo | null = null;
  resampling: [number, number] | null = null;

  private snap: EngineSnapshot = { status: 'idle', error: null, uid: null, volume: 1, muted: false, transcoded: false, dsp: null, sid: null };
  private time = 0;
  private dur = 0;
  private pos = 0;
  private posAt = 0;
  private lastEvent = -1;
  /** 지금 곡과 다음 곡, 그리고 각각에 보낸 DSP 요청 */
  private current: QueueItem | null = null;
  private next: QueueItem | null = null;
  private sent = new Map<string, string | undefined>();
  private listeners = new Set<Listener>();
  private timeListeners = new Set<Listener>();
  private es: EventSource | null = null;
  private timer: ReturnType<typeof setInterval> | null = null;

  constructor() {
    if (typeof window === 'undefined') return;
    this.es = new EventSource('/api/player/events');
    this.es.addEventListener('player', (e) => this.apply(JSON.parse((e as MessageEvent<string>).data) as ServerState));
    this.timer = setInterval(() => this.tick(), 200);
  }

  dispose() {
    this.es?.close();
    if (this.timer) clearInterval(this.timer);
  }

  subscribe = (l: Listener) => {
    this.listeners.add(l);
    return () => this.listeners.delete(l);
  };
  getSnapshot = () => this.snap;
  subscribeTime = (l: Listener) => {
    this.timeListeners.add(l);
    return () => this.timeListeners.delete(l);
  };
  getTime = () => this.time;
  getDuration = () => this.dur;

  private update(patch: Partial<EngineSnapshot>) {
    const next = { ...this.snap, ...patch };
    if (Object.keys(patch).every((k) => next[k as keyof EngineSnapshot] === this.snap[k as keyof EngineSnapshot])) return;
    this.snap = next;
    for (const l of this.listeners) l();
  }

  private emitTime() {
    for (const l of this.timeListeners) l();
  }

  private tick() {
    if (this.snap.status !== 'playing') return;
    const t = this.pos + (performance.now() - this.posAt) / 1000;
    this.time = this.dur > 0 ? Math.min(t, this.dur) : t;
    this.emitTime();
  }

  private apply(st: ServerState) {
    // 처음 붙었을 때 이미 지나간 사건은 다시 처리하지 않는다.
    const newest = st.events.length ? st.events[st.events.length - 1]![0] : 0;
    if (this.lastEvent < 0) this.lastEvent = newest;
    for (const [seq, ev] of st.events) {
      if (seq <= this.lastEvent) continue;
      this.lastEvent = seq;
      if (ev.type === 'advanced' && ev.uid) {
        if (this.next?.uid === ev.uid) {
          this.current = this.next;
          this.next = null;
          const plan = this.plan(this.current);
          this.update({ dsp: plan?.req ?? null, sid: plan ? this.sidOf(this.current) : null });
        }
        this.onAdvanced?.(ev.uid);
      }
      else if (ev.type === 'ended') this.onEnded?.();
      else if (ev.type === 'error') this.onError?.(ev.message ?? '재생하지 못했습니다');
    }
    this.output = st.output;
    this.source = st.source;
    this.resampling = st.resampling;
    this.pos = st.position;
    this.posAt = performance.now();
    this.time = st.position;
    this.dur = st.duration || this.dur;
    this.update({ status: st.status, uid: st.uid, volume: st.volume, muted: st.muted, error: st.error });
    // 장치·원본 정보가 바뀌면 신호 경로를 다시 그리게 한다.
    for (const l of this.listeners) l();
    this.emitTime();
  }

  sidOf(item: QueueItem) {
    return `native.${item.uid}`;
  }

  private plan(item: QueueItem): DspPlan | null {
    return this.dspFor?.(item) ?? null;
  }

  load(item: QueueItem, opts: { autoplay: boolean; at?: number }) {
    const at = opts.at ?? 0;
    const plan = this.plan(item);
    this.current = item;
    this.sent.set(item.uid, plan?.q);
    void post('/api/player/load', { item: toPlayItem(item, plan), autoplay: opts.autoplay, at, force: at > 0 });
    this.update({ dsp: plan?.req ?? null, sid: plan ? this.sidOf(item) : null });
    if (this.snap.uid !== item.uid) {
      this.time = at;
      this.pos = at;
      this.posAt = performance.now();
      this.dur = item.track.duration;
      this.update({ uid: item.uid, status: opts.autoplay ? 'loading' : 'paused', error: null });
      this.emitTime();
    }
  }

  preload(item: QueueItem | null) {
    this.next = item;
    const plan = item ? this.plan(item) : null;
    if (item) this.sent.set(item.uid, plan?.q);
    void post('/api/player/preload', { item: item ? toPlayItem(item, plan) : null });
  }

  /** DSP 설정이 바뀌었다. 지금 곡과 다음 곡의 요청을 다시 만들어 바뀐 것만 보낸다. */
  refreshDsp() {
    const items: { uid: string; dsp: string | null }[] = [];
    for (const it of [this.current, this.next]) {
      if (!it) continue;
      const q = this.plan(it)?.q;
      if (this.sent.get(it.uid) === q) continue;
      this.sent.set(it.uid, q);
      items.push({ uid: it.uid, dsp: q ?? null });
    }
    if (this.current) {
      const plan = this.plan(this.current);
      this.update({ dsp: plan?.req ?? null, sid: plan ? this.sidOf(this.current) : null });
    }
    if (items.length) void post('/api/player/dsp', { items });
  }

  play() {
    if (!this.snap.uid) return;
    this.update({ status: 'loading' });
    void post('/api/player/control', { cmd: 'play' });
  }

  pause() {
    this.update({ status: this.snap.uid ? 'paused' : 'idle' });
    void post('/api/player/control', { cmd: 'pause' });
  }

  toggle() {
    if (this.playing) this.pause();
    else this.play();
  }

  seek(t: number) {
    const target = Math.max(0, this.dur > 0 ? Math.min(t, this.dur - 0.05) : t);
    this.time = target;
    this.pos = target;
    this.posAt = performance.now();
    this.emitTime();
    void post('/api/player/control', { cmd: 'seek', t: target });
  }

  stop() {
    this.update({ status: 'idle', uid: null });
    this.time = 0;
    this.emitTime();
    void post('/api/player/control', { cmd: 'stop' });
  }

  setVolume(v: number) {
    const volume = Math.min(1, Math.max(0, v));
    this.update({ volume, muted: false });
    void post('/api/player/control', { cmd: 'volume', volume, muted: false });
  }

  setMuted(m: boolean) {
    this.update({ muted: m });
    void post('/api/player/control', { cmd: 'volume', muted: m });
  }

  get playing() {
    return this.snap.status === 'playing' || this.snap.status === 'loading';
  }

  get currentTime() {
    return this.time;
  }

  get duration() {
    return this.dur;
  }
}
