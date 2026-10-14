/**
 * 소리를 내는 부분. React를 모른다.
 *
 * 오디오 요소를 두 개 둔다. 하나가 지금 곡을 틀고, 다른 하나는 다음 곡을 미리 받아
 * 둔다. 곡이 끝나면 역할만 바꿔 곧바로 이어 튼다 — 다음 곡을 그때 가서 요청하는 것보다
 * 곡 사이의 빈틈이 훨씬 짧다.
 *
 * 브라우저가 못 여는 형식은 서버가 FLAC으로 바꿔 흘려 보낸다. 그런 스트림은 길이를
 * 모르고 탐색도 안 되므로, 탐색할 때는 그 위치부터 다시 요청하고 시작점을 `offset`으로
 * 기억해 둔다.
 */
import type { DspRequest } from '../dsp/types';
import type { QueueItem } from './types';

export type EngineStatus = 'idle' | 'loading' | 'playing' | 'paused' | 'error';

export interface EngineSnapshot {
  status: EngineStatus;
  error: string | null;
  uid: string | null;
  volume: number;
  muted: boolean;
  /** 지금 곡이 서버에서 변환되어 오고 있는지. 신호 경로에 그대로 적는다. */
  transcoded: boolean;
  /** 지금 곡에 걸린 DSP 요청. 없으면 원본 그대로 */
  dsp: DspRequest | null;
  /** 클리핑 표시기 세션 */
  sid: string | null;
}

export interface DspPlan {
  /** 주소에 넣을 값 */
  q: string;
  req: DspRequest;
}

interface Deck {
  el: HTMLAudioElement;
  item: QueueItem | null;
  offset: number;
  transcoded: boolean;
  dsp: DspPlan | null;
  failed: boolean;
  /** 늦게 도착한 이벤트가 새 곡에 작용하지 않게. */
  token: number;
}

const playable = new Map<string, boolean>();

export function canPlayNatively(mime: string): boolean {
  if (typeof document === 'undefined') return true;
  let ok = playable.get(mime);
  if (ok === undefined) {
    ok = document.createElement('audio').canPlayType(mime) !== '';
    playable.set(mime, ok);
  }
  return ok;
}

type Listener = () => void;

/**
 * 소리를 내는 쪽의 공통 모양. 이 브라우저의 `<audio>`(AudioEngine)든, 소리 서버가 이 기기의
 * 오디오 장치로 직접 트는 것(NativeEngine)이든 플레이어는 같은 방식으로 다룬다.
 */
export interface PlaybackEngine {
  readonly kind: 'browser' | 'native';
  readonly zone: string;
  canTranscode: boolean;
  onEnded: (() => void) | null;
  onError: ((message: string) => void) | null;
  /** 엔진이 스스로 다음 곡으로 넘어갔다(갭리스). 화면은 대기열 위치만 맞춘다. */
  onAdvanced: ((uid: string) => void) | null;
  dspFor: ((item: QueueItem) => DspPlan | null) | null;
  subscribe(l: Listener): () => void;
  getSnapshot(): EngineSnapshot;
  subscribeTime(l: Listener): () => void;
  getTime(): number;
  getDuration(): number;
  sidOf(item: QueueItem): string;
  load(item: QueueItem, opts: { autoplay: boolean; at?: number }): void;
  preload(item: QueueItem | null): void;
  refreshDsp(): void;
  play(): void;
  pause(): void;
  toggle(): void;
  seek(t: number): void;
  stop(): void;
  setVolume(v: number): void;
  setMuted(m: boolean): void;
  readonly playing: boolean;
  readonly currentTime: number;
  readonly duration: number;
}

export class AudioEngine implements PlaybackEngine {
  readonly kind = 'browser' as const;
  canTranscode = false;
  onEnded: (() => void) | null = null;
  onError: ((message: string) => void) | null = null;
  onAdvanced: ((uid: string) => void) | null = null;
  /** 곡마다 DSP를 어떻게 걸지. 프로바이더가 설정을 보고 정한다. null이면 원본 그대로. */
  dspFor: ((item: QueueItem) => DspPlan | null) | null = null;
  /** 이 페이지(존)의 이름. 클리핑 세션 ID의 앞부분. */
  readonly zone = Math.random().toString(36).slice(2, 10);

  private _decks: [Deck, Deck] | null = null;
  private a = 0;
  private wantPlay = false;
  private listeners = new Set<Listener>();
  private timeListeners = new Set<Listener>();
  private snap: EngineSnapshot = {
    status: 'idle',
    error: null,
    uid: null,
    volume: 1,
    muted: false,
    transcoded: false,
    dsp: null,
    sid: null,
  };
  private time = 0;
  private dur = 0;

  // -- 구독 (useSyncExternalStore) -------------------------------------------

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
    if ((Object.keys(patch) as (keyof EngineSnapshot)[]).every((k) => next[k] === this.snap[k])) return;
    this.snap = next;
    for (const l of this.listeners) l();
  }

  private emitTime() {
    const t = this.currentTime;
    const d = this.duration;
    if (t === this.time && d === this.dur) return;
    this.time = t;
    this.dur = d;
    for (const l of this.timeListeners) l();
  }

  // -- 덱 ---------------------------------------------------------------------

  private get decks(): [Deck, Deck] {
    if (!this._decks) {
      const make = (): Deck => {
        const el = new Audio();
        el.preload = 'auto';
        return { el, item: null, offset: 0, transcoded: false, dsp: null, failed: false, token: 0 };
      };
      this._decks = [make(), make()];
      for (const d of this._decks) this.bind(d);
      for (const d of this._decks) {
        d.el.volume = this.snap.volume;
        d.el.muted = this.snap.muted;
      }
    }
    return this._decks;
  }

  private get active(): Deck {
    return this.decks[this.a]!;
  }

  private get standby(): Deck {
    return this.decks[1 - this.a]!;
  }

  private bind(d: Deck) {
    const el = d.el;
    const mine = () => d === this.active && d.item !== null;
    el.addEventListener('timeupdate', () => mine() && this.emitTime());
    el.addEventListener('durationchange', () => mine() && this.emitTime());
    el.addEventListener('loadedmetadata', () => mine() && this.emitTime());
    el.addEventListener('playing', () => mine() && this.update({ status: 'playing', error: null }));
    el.addEventListener('waiting', () => mine() && this.wantPlay && this.update({ status: 'loading' }));
    el.addEventListener('pause', () => mine() && !el.ended && this.update({ status: 'paused' }));
    el.addEventListener('canplay', () => {
      if (mine() && el.paused) this.update({ status: this.wantPlay ? 'loading' : 'paused' });
    });
    el.addEventListener('ended', () => {
      if (!mine()) return;
      this.emitTime();
      this.onEnded?.();
    });
    el.addEventListener('error', () => {
      if (!d.item) return;
      d.failed = true;
      if (d !== this.active) return;
      const message = this.describeError(d);
      this.update({ status: 'error', error: message });
      this.onError?.(message);
    });
  }

  private describeError(d: Deck): string {
    const t = d.item!.track;
    const code = d.el.error?.code;
    if (!canPlayNatively(t.format.mime) && !d.transcoded) {
      return t.transcodeSrc
        ? `이 브라우저는 ${t.format.codec}를 재생하지 못합니다. 서버에 ffmpeg를 설치하면 변환해서 재생합니다.`
        : `이 브라우저는 ${t.format.codec}를 재생하지 못합니다.`;
    }
    if (code === MediaError.MEDIA_ERR_NETWORK) return '네트워크가 끊겨 재생을 멈췄습니다.';
    if (code === MediaError.MEDIA_ERR_DECODE) return '파일을 해독하지 못했습니다. 파일이 손상되었을 수 있습니다.';
    return `"${t.title}"을(를) 불러오지 못했습니다.`;
  }

  /** 원본으로 갈지, 변환을 거칠지. */
  sidOf(item: QueueItem) {
    return `${this.zone}.${item.uid}`;
  }

  private plan(item: QueueItem, at: number): { url: string; transcoded: boolean; dsp: DspPlan | null } {
    const t = item.track;
    const dsp = t.dspSrc && this.canTranscode ? (this.dspFor?.(item) ?? null) : null;
    if (dsp) {
      return { url: `${t.dspSrc}dsp=${dsp.q}&sid=${this.sidOf(item)}${at > 0 ? `&t=${at.toFixed(2)}` : ''}`, transcoded: true, dsp };
    }
    if (!canPlayNatively(t.format.mime) && t.transcodeSrc && this.canTranscode) {
      return { url: `${t.transcodeSrc}${at > 0 ? `&t=${at.toFixed(2)}` : ''}`, transcoded: true, dsp: null };
    }
    return { url: t.src, transcoded: false, dsp: null };
  }

  private setDeck(d: Deck, item: QueueItem, at: number) {
    const token = ++d.token;
    const p = this.plan(item, at);
    d.item = item;
    d.failed = false;
    d.transcoded = p.transcoded;
    d.dsp = p.dsp;
    d.offset = p.transcoded ? at : 0;
    d.el.src = p.url;
    if (!p.transcoded && at > 0) {
      d.el.addEventListener(
        'loadedmetadata',
        () => {
          if (d.token !== token) return;
          try {
            d.el.currentTime = at;
          } catch {
            // 탐색할 수 없는 스트림
          }
        },
        { once: true },
      );
    }
  }

  private clearDeck(d: Deck) {
    d.token++;
    d.item = null;
    d.dsp = null;
    d.failed = false;
    d.el.pause();
    d.el.removeAttribute('src');
    d.el.load();
  }

  private playActive() {
    const d = this.active;
    if (!d.item) return;
    d.el.play().catch((e: DOMException) => {
      if (d !== this.active) return;
      // 자동 재생이 막혔거나(사용자가 아직 아무것도 누르지 않음) 새 곡에 밀려 중단됨.
      if (e.name === 'NotAllowedError') {
        this.wantPlay = false;
        this.update({ status: 'paused' });
      }
    });
  }

  // -- 바깥에서 부르는 것 -----------------------------------------------------

  /** 곡을 건다. 다음 곡으로 미리 받아 둔 것이면 덱만 바꾼다. */
  load(item: QueueItem, opts: { autoplay: boolean; at?: number }) {
    const at = opts.at ?? 0;
    this.wantPlay = opts.autoplay;
    const sb = this.standby;
    if (sb.item?.uid === item.uid && !sb.failed && at === 0) {
      const old = this.active;
      this.a = 1 - this.a;
      if (old.item) this.clearDeck(old);
      if (this.active.el.currentTime > 0) this.active.el.currentTime = 0;
    } else {
      this.setDeck(this.active, item, at);
    }
    this.update({
      uid: item.uid,
      transcoded: this.active.transcoded,
      dsp: this.active.dsp?.req ?? null,
      sid: this.active.dsp ? this.sidOf(item) : null,
      status: opts.autoplay ? 'loading' : 'paused',
      error: null,
    });
    this.time = at;
    this.dur = item.track.duration;
    for (const l of this.timeListeners) l();
    if (opts.autoplay) this.playActive();
  }

  /** 다음 곡을 미리 받아 둔다. null이면 받아 둔 것을 버린다. */
  preload(item: QueueItem | null) {
    const sb = this.standby;
    if (!item) {
      if (sb.item) this.clearDeck(sb);
      return;
    }
    if (sb.item?.uid === item.uid && !sb.failed) return;
    this.setDeck(sb, item, 0);
  }

  /**
   * DSP 설정이 바뀌었다. 지금 곡의 요청이 달라졌으면 지금 위치에서 다시 걸고,
   * 미리 받아 둔 다음 곡도 새 설정으로 다시 받는다.
   */
  refreshDsp() {
    if (!this._decks) return;
    const d = this.active;
    if (d.item) {
      const next = d.item.track.dspSrc && this.canTranscode ? (this.dspFor?.(d.item) ?? null) : null;
      if ((next?.q ?? null) !== (d.dsp?.q ?? null)) {
        const at = this.currentTime;
        this.setDeck(d, d.item, at);
        this.update({ transcoded: d.transcoded, dsp: d.dsp?.req ?? null, sid: d.dsp ? this.sidOf(d.item) : null });
        this.time = at;
        if (this.wantPlay) this.playActive();
      }
    }
    const sb = this.standby;
    if (sb.item) {
      const next = sb.item.track.dspSrc && this.canTranscode ? (this.dspFor?.(sb.item) ?? null) : null;
      if ((next?.q ?? null) !== (sb.dsp?.q ?? null)) this.setDeck(sb, sb.item, 0);
    }
  }

  play() {
    const d = this.active;
    if (!d.item) return;
    this.wantPlay = true;
    if (d.failed) {
      this.setDeck(d, d.item, this.currentTime);
      this.update({
        status: 'loading',
        error: null,
        transcoded: d.transcoded,
        dsp: d.dsp?.req ?? null,
        sid: d.dsp ? this.sidOf(d.item) : null,
      });
    } else if (d.el.paused) {
      this.update({ status: 'loading' });
    }
    this.playActive();
  }

  pause() {
    this.wantPlay = false;
    if (this._decks) this.active.el.pause();
    this.update({ status: this.snap.uid ? 'paused' : 'idle' });
  }

  toggle() {
    const s = this.snap.status;
    if (s === 'playing' || (this.wantPlay && s === 'loading')) this.pause();
    else this.play();
  }

  seek(t: number) {
    const d = this.active;
    if (!d.item) return;
    const max = this.duration || d.item.track.duration;
    t = Math.max(0, Math.min(t, max > 1 ? max - 0.5 : max));
    if (d.transcoded) {
      this.setDeck(d, d.item, t);
      if (this.wantPlay) this.playActive();
    } else {
      try {
        d.el.currentTime = t;
      } catch {
        // 메타데이터가 오기 전
      }
    }
    this.time = t;
    for (const l of this.timeListeners) l();
  }

  stop() {
    this.wantPlay = false;
    if (this._decks) for (const d of this._decks) if (d.item) this.clearDeck(d);
    this.time = 0;
    this.dur = 0;
    this.update({ status: 'idle', uid: null, error: null, transcoded: false, dsp: null, sid: null });
    for (const l of this.timeListeners) l();
  }

  setVolume(v: number) {
    v = Math.max(0, Math.min(1, v));
    if (this._decks) for (const d of this._decks) d.el.volume = v;
    this.update({ volume: v, muted: v === 0 ? this.snap.muted : false });
    if (v > 0 && this._decks) for (const d of this._decks) d.el.muted = false;
  }

  setMuted(m: boolean) {
    if (this._decks) for (const d of this._decks) d.el.muted = m;
    this.update({ muted: m });
  }

  get playing() {
    return this.wantPlay;
  }

  get currentTime() {
    if (!this._decks) return 0;
    const d = this.active;
    return d.item ? d.offset + (d.el.currentTime || 0) : 0;
  }

  get duration() {
    if (!this._decks) return 0;
    const d = this.active;
    if (!d.item) return 0;
    if (!d.transcoded && Number.isFinite(d.el.duration) && d.el.duration > 0) return d.el.duration;
    return d.item.track.duration;
  }
}
