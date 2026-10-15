'use client';

/**
 * 대기열과 재생 상태.
 *
 * 컨텍스트를 둘로 나눴다. 버튼들이 쓰는 동작(`usePlayer`)은 한 번 만들어지면 바뀌지
 * 않고, 대기열(`useQueue`)은 바뀔 때마다 그걸 보는 곳만 다시 그린다. 재생 위치처럼
 * 1초에 여러 번 바뀌는 값은 엔진을 직접 구독한다(`usePlaybackTime`).
 */
import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useLayoutEffect,
  useMemo,
  useReducer,
  useRef,
  useState,
  useSyncExternalStore,
  type ReactNode,
} from 'react';
import { useToast } from '@/components/toast';
import type { Caps } from '../dsp/plan';
import { buildRequest, encodeRequest } from '../dsp/request';
import { useDsp } from '../dsp/store';
import type { AudioFormat } from '../library/types';
import { AudioEngine, type EngineSnapshot, type PlaybackEngine } from './engine';
import { NativeEngine } from './native';
import { pushRecent, readJSON, writeJSON } from './storage';
import { initialQueue as initial, nextIndex, queueReducer as reducer, queueWindows, wrap, type QueueState } from './queue';
import type { PlayableTrack, QueueItem, RepeatMode } from './types';

// -- 컨텍스트 ---------------------------------------------------------------

export type ZoneKind = 'browser' | 'native';

export interface PlayerActions {
  engine: PlaybackEngine;
  /** 소리를 낼 곳을 바꾼다. 지금 위치에서 이어 튼다. */
  setZone(zone: ZoneKind): void;
  playTracks(tracks: PlayableTrack[], start?: number, opts?: { shuffle?: boolean }): void;
  playNext(tracks: PlayableTrack[]): void;
  enqueue(tracks: PlayableTrack[]): void;
  jump(index: number): void;
  next(): void;
  prev(): void;
  remove(index: number): void;
  move(from: number, to: number): void;
  clearUpcoming(): void;
  clearAll(): void;
  toggleShuffle(): void;
  cycleRepeat(): void;
  togglePlay(): void;
  seek(t: number): void;
  setVolume(v: number): void;
  toggleMute(): void;
}

export interface QueueView {
  items: QueueItem[];
  index: number;
  current: QueueItem | null;
  shuffle: boolean;
  repeat: RepeatMode;
  /** 기기의 출력 샘플레이트. 첫 재생 때 잰다. */
  outputRate: number | null;
  canTranscode: boolean;
  /** 서버 ffmpeg가 할 수 있는 것(SoX, bs2b). 신호 경로 표시가 쓴다. */
  caps: Caps;
  /** 라이브 FLAC처럼 나중에 알게 된 값까지 합친 포맷. */
  formatOf(track: PlayableTrack): AudioFormat;
}

export interface ZoneView {
  zone: ZoneKind;
  /** 소리 서버가 이 기기의 오디오 장치로 직접 틀 수 있는지 */
  nativeAvailable: boolean;
}

interface Ui {
  nowPlaying: boolean;
  setNowPlaying(open: boolean): void;
  queue: boolean;
  setQueue(open: boolean): void;
}

const ActionsCtx = createContext<PlayerActions | null>(null);
const QueueCtx = createContext<QueueView | null>(null);
const UiCtx = createContext<Ui | null>(null);
const ZoneCtx = createContext<ZoneView | null>(null);

export function useZone(): ZoneView {
  const v = useContext(ZoneCtx);
  if (!v) throw new Error('PlayerProvider 밖');
  return v;
}

export function usePlayer(): PlayerActions {
  const v = useContext(ActionsCtx);
  if (!v) throw new Error('PlayerProvider 밖');
  return v;
}

export function useQueue(): QueueView {
  const v = useContext(QueueCtx);
  if (!v) throw new Error('PlayerProvider 밖');
  return v;
}

export function useUi(): Ui {
  const v = useContext(UiCtx);
  if (!v) throw new Error('PlayerProvider 밖');
  return v;
}

const SERVER_SNAPSHOT: EngineSnapshot = {
  status: 'idle',
  error: null,
  uid: null,
  volume: 1,
  muted: false,
  transcoded: false,
  dsp: null,
  sid: null,
};

export function useEngineState(): EngineSnapshot {
  const { engine } = usePlayer();
  return useSyncExternalStore(engine.subscribe, engine.getSnapshot, () => SERVER_SNAPSHOT);
}

export function usePlaybackTime(): number {
  const { engine } = usePlayer();
  return useSyncExternalStore(engine.subscribeTime, engine.getTime, () => 0);
}

export function usePlaybackDuration(): number {
  const { engine } = usePlayer();
  return useSyncExternalStore(engine.subscribeTime, engine.getDuration, () => 0);
}

/** 지금 이 곡을 틀고 있는지. 곡 목록의 막대 표시에 쓴다. */
export function useIsCurrent(key: string): 'playing' | 'paused' | null {
  const { current } = useQueue();
  const { status } = useEngineState();
  if (current?.track.key !== key) return null;
  return status === 'playing' || status === 'loading' ? 'playing' : 'paused';
}

// -- 프로바이더 -------------------------------------------------------------

interface Saved {
  items: QueueItem[];
  index: number;
  shuffle: boolean;
  repeat: RepeatMode;
  unshuffled: QueueItem[] | null;
}

const MAX_SAVED = 1500;

export function PlayerProvider({
  children,
  canTranscode,
  caps,
  preferNative = false,
}: {
  children: ReactNode;
  canTranscode: boolean;
  caps: Caps;
  /** 앱 안에서는 처음부터 이 기기의 장치로 튼다. */
  preferNative?: boolean;
}) {
  const toast = useToast();
  const [browser] = useState(() => new AudioEngine());
  browser.canTranscode = canTranscode;
  const nativeRef = useRef<NativeEngine | null>(null);
  const [zone, setZoneState] = useState<ZoneKind>('browser');
  const [nativeAvailable, setNativeAvailable] = useState(false);

  // 소리 서버가 이 기기의 오디오 장치를 갖고 있는지 묻는다.
  useEffect(() => {
    let alive = true;
    fetch('/api/player', { cache: 'no-store' })
      .then((r) => (r.ok ? r.json() : null))
      .then((j: { available?: boolean } | null) => {
        if (!alive || !j?.available) return;
        setNativeAvailable(true);
        const saved = readJSON<ZoneKind>('sori.zone');
        if (saved === 'native' || (saved == null && preferNative)) setZoneState('native');
      })
      .catch(() => {});
    return () => {
      alive = false;
    };
  }, [preferNative]);

  const engine: PlaybackEngine = useMemo(() => {
    if (zone === 'native' && nativeAvailable) {
      nativeRef.current ??= new NativeEngine();
      return nativeRef.current;
    }
    return browser;
  }, [zone, nativeAvailable, browser]);
  const engineRef = useRef(engine);
  useLayoutEffect(() => {
    engineRef.current = engine;
  });

  const [state, dispatch] = useReducer(reducer, initial);
  const stateRef = useRef(state);
  useLayoutEffect(() => {
    stateRef.current = state;
  });

  /** 다음에 곡을 걸 때 바로 틀지, 어디서부터 틀지. 동작마다 정해 두고 한 번 쓴다. */
  const loadReq = useRef<{ autoplay: boolean; at: number } | null>(null);
  const restored = useRef(false);
  const [outputRate, setOutputRate] = useState<number | null>(null);
  const [probes, setProbes] = useState<Record<string, Partial<AudioFormat>>>({});
  const [nowPlaying, setNowPlaying] = useState(false);
  const [queueOpen, setQueue] = useState(false);

  const current = state.items[state.index] ?? null;

  const outputRateRef = useRef<number | null>(null);
  const measureOutput = useCallback(() => {
    if (outputRateRef.current) return;
    try {
      const Ctx = window.AudioContext ?? (window as unknown as { webkitAudioContext: typeof AudioContext }).webkitAudioContext;
      const ctx = new Ctx();
      outputRateRef.current = ctx.sampleRate;
      setOutputRate(ctx.sampleRate);
      void ctx.close();
    } catch {
      // 모르는 채로 둔다
    }
  }, []);
  const measureRef = useRef(measureOutput);
  useLayoutEffect(() => {
    measureRef.current = measureOutput;
  });

  // 출력 샘플레이트는 사용자가 처음 뭔가를 누를 때 잰다. 그 전에 오디오 컨텍스트를 만들면
  // 브라우저가 자동 재생 경고를 띄운다. 이어 듣기로 곡이 걸려 있어도 신호 경로가 곧 정확해진다.
  useEffect(() => {
    const once = () => measureRef.current();
    window.addEventListener('pointerdown', once, { once: true, capture: true });
    window.addEventListener('keydown', once, { once: true, capture: true });
    return () => {
      window.removeEventListener('pointerdown', once, { capture: true });
      window.removeEventListener('keydown', once, { capture: true });
    };
  }, []);

  // 볼륨은 존을 바꿔도 따라간다.
  useEffect(() => {
    const vol = readJSON<{ volume: number; muted: boolean }>('sori.volume');
    if (vol) {
      engine.setVolume(vol.volume);
      engine.setMuted(vol.muted);
    }
  }, [engine]);

  // 이어 듣기: 지난번 대기열과 위치. 처음 한 번만.
  useEffect(() => {
    const saved = readJSON<Saved>('sori.queue');
    if (saved?.items?.length) {
      const index = Math.min(Math.max(0, saved.index), saved.items.length - 1);
      const pos = readJSON<{ uid: string; t: number }>('sori.position');
      loadReq.current = { autoplay: false, at: pos && pos.uid === saved.items[index]!.uid ? pos.t : 0 };
      dispatch({
        type: 'restore',
        state: {
          items: saved.items,
          index,
          shuffle: !!saved.shuffle,
          repeat: saved.repeat ?? 'off',
          unshuffled: saved.unshuffled ?? null,
          token: 1,
        },
      });
    }
    restored.current = true;
  }, []);

  // 지금 곡을 건다.
  useEffect(() => {
    if (!current) {
      engine.stop();
      return;
    }
    const req = loadReq.current ?? { autoplay: engine.playing, at: 0 };
    loadReq.current = null;
    engine.load(current, req);
    if (req.autoplay) {
      const t = current.track;
      if (t.source === 'library' && t.albumId) {
        pushRecent({ kind: 'album', id: t.albumId, title: t.album, artist: t.art.subtitle ?? t.artist, cover: t.cover, seed: t.art.seed });
      } else if (t.source === 'live' && t.probe) {
        pushRecent({ kind: 'live', id: t.probe.id, title: t.artist, artist: t.art.subtitle ?? '', seed: t.art.seed });
      }
    }
    // uid와 token이 바뀔 때만. 대기열의 다른 곳이 바뀌어도 지금 곡을 다시 걸지 않는다.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [engine, current?.uid, state.token]);

  // 다음 곡을 미리 받아 둔다. 이 기기 존은 앞뒤 대기열을 통째로 맡겨 화면 없이도 이어 튼다.
  useEffect(() => {
    if (engine.setQueue) {
      const w = queueWindows(state);
      engine.setQueue(w.upcoming, w.previous, w.repeatOne);
      return;
    }
    const n = state.repeat === 'one' ? null : nextIndex(state);
    const next = n == null ? null : state.items[n]!;
    engine.preload(next && next.uid !== current?.uid ? next : null);
  }, [engine, state, current?.uid]);

  // DSP: 곡마다 요청을 만든다. 설정은 기기에 저장된 것, 레벨링 "자동"은 앞뒤 곡을 본다.
  const { config: dsp } = useDsp();
  const dspRef = useRef(dsp);
  const probesRef = useRef<Record<string, Partial<AudioFormat>>>({});
  useLayoutEffect(() => {
    dspRef.current = dsp;
  });
  useEffect(() => {
    engine.dspFor = (item) => {
      const s = stateRef.current;
      const i = s.items.findIndex((x) => x.uid === item.uid);
      const albumId = item.track.albumId;
      const same = (j: number) => !!albumId && s.items[j]?.track.albumId === albumId;
      const sourceRate = probesRef.current[item.track.key]?.sampleRate ?? item.track.format.sampleRate;
      // 이 기기의 장치로 틀 때는 그 장치의 샘플레이트가 기준이다.
      const deviceRate = engine instanceof NativeEngine ? (engine.output?.rate ?? null) : outputRateRef.current;
      const req = buildRequest(dspRef.current, {
        sourceRate,
        deviceRate,
        sameAlbumNeighbors: i >= 0 && (same(i - 1) || same(i + 1)),
      });
      return req ? { q: encodeRequest(req), req } : null;
    };
    return () => {
      engine.dspFor = null;
    };
  }, [engine]);
  // 설정을 바꾸면 지금 위치에서 다시 건다. 슬라이더를 끄는 동안 매번 다시 걸지 않게 잠깐 기다린다.
  const firstDsp = useRef(true);
  useEffect(() => {
    if (firstDsp.current) {
      firstDsp.current = false;
      return;
    }
    const t = setTimeout(() => engine.refreshDsp(), 350);
    return () => clearTimeout(t);
  }, [engine, dsp, outputRate]);

  // 대기열 저장
  useEffect(() => {
    if (!restored.current) return;
    const start = state.items.length > MAX_SAVED ? Math.max(0, state.index - 50) : 0;
    writeJSON('sori.queue', {
      items: state.items.slice(start, start + MAX_SAVED),
      index: state.index - start,
      shuffle: state.shuffle,
      repeat: state.repeat,
      unshuffled: state.unshuffled && state.unshuffled.length <= MAX_SAVED ? state.unshuffled : null,
    } satisfies Saved);
  }, [state]);

  // 볼륨과 위치 저장
  useEffect(() => {
    let last = engine.getSnapshot();
    const off = engine.subscribe(() => {
      const s = engine.getSnapshot();
      if (s.volume !== last.volume || s.muted !== last.muted) writeJSON('sori.volume', { volume: s.volume, muted: s.muted });
      last = s;
    });
    const savePos = () => {
      const uid = engine.getSnapshot().uid;
      if (uid) writeJSON('sori.position', { uid, t: engine.currentTime });
    };
    const timer = setInterval(savePos, 4000);
    window.addEventListener('pagehide', savePos);
    return () => {
      off();
      clearInterval(timer);
      window.removeEventListener('pagehide', savePos);
    };
  }, [engine]);

  // 곡이 끝나면 / 실패하면
  useEffect(() => {
    engine.onEnded = () => {
      const s = stateRef.current;
      if (s.repeat === 'one') {
        engine.seek(0);
        engine.play();
        return;
      }
      const n = nextIndex(s);
      if (n == null) {
        engine.pause();
        engine.seek(0);
        return;
      }
      loadReq.current = { autoplay: true, at: 0 };
      dispatch({ type: 'jump', index: n });
    };
    engine.onError = (message) => {
      toast(message, 'error');
      const s = stateRef.current;
      const uid = s.items[s.index]?.uid;
      const n = nextIndex(s);
      if (n == null || n === s.index) return;
      // 잠깐 보여 주고 다음 곡으로. 그 사이 사용자가 다른 걸 골랐으면 그대로 둔다.
      setTimeout(() => {
        const now = stateRef.current;
        if (now.items[now.index]?.uid !== uid || engine.getSnapshot().status !== 'error') return;
        loadReq.current = { autoplay: true, at: 0 };
        dispatch({ type: 'jump', index: n });
      }, 1800);
    };
    return () => {
      engine.onEnded = null;
      engine.onError = null;
    };
  }, [engine, toast]);

  // 엔진이 스스로 다음 곡으로 넘어갔다(네이티브 존의 갭리스). 대기열 위치만 맞춘다.
  useEffect(() => {
    engine.onAdvanced = (uid) => {
      const s = stateRef.current;
      const i = s.items.findIndex((x) => x.uid === uid);
      if (i < 0 || i === s.index) return;
      loadReq.current = { autoplay: true, at: 0 };
      dispatch({ type: 'jump', index: i });
    };
    return () => {
      engine.onAdvanced = null;
    };
  }, [engine]);

  // 라이브 FLAC의 실제 샘플레이트
  const probeKey = current?.track.probe && !current.track.format.sampleRate ? current.track.key : null;
  useEffect(() => {
    if (!probeKey || probes[probeKey]) return;
    const p = stateRef.current.items.find((i) => i.track.key === probeKey)?.track.probe;
    if (!p) return;
    let cancelled = false;
    fetch(`/api/live/probe?id=${encodeURIComponent(p.id)}&file=${encodeURIComponent(p.file)}`)
      .then((r) => (r.ok ? r.json() : null))
      .then((info: { sampleRate: number; bitDepth: number; channels: number } | null) => {
        if (cancelled || !info) return;
        const patch = { sampleRate: info.sampleRate, bitDepth: info.bitDepth, channels: info.channels };
        probesRef.current = { ...probesRef.current, [probeKey]: patch };
        setProbes((ps) => ({ ...ps, [probeKey]: patch }));
      })
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  }, [probeKey, probes]);

  const actions = useMemo<PlayerActions>(() => {
    const begin = () => {
      measureRef.current();
      loadReq.current = { autoplay: true, at: 0 };
    };
    return {
      engine,
      setZone(next) {
        const old = engineRef.current;
        if (old.kind === next) return;
        const at = old.currentTime;
        const was = old.playing;
        old.pause();
        if (stateRef.current.items.length) loadReq.current = { autoplay: was, at };
        writeJSON('sori.zone', next);
        setZoneState(next);
      },
      playTracks(tracks, start = 0, opts) {
        if (!tracks.length) return;
        begin();
        dispatch({ type: 'replace', items: wrap(tracks), index: start, shuffle: opts?.shuffle });
      },
      playNext(tracks) {
        if (!tracks.length) return;
        if (!stateRef.current.items.length) begin();
        dispatch({ type: 'insertNext', items: wrap(tracks) });
        toast(tracks.length === 1 ? `다음 곡: ${tracks[0]!.title}` : `${tracks.length}곡을 다음에 재생합니다`);
      },
      enqueue(tracks) {
        if (!tracks.length) return;
        if (!stateRef.current.items.length) begin();
        dispatch({ type: 'append', items: wrap(tracks) });
        toast(tracks.length === 1 ? `대기열에 추가: ${tracks[0]!.title}` : `${tracks.length}곡을 대기열에 추가했습니다`);
      },
      jump(index) {
        begin();
        dispatch({ type: 'jump', index });
      },
      next() {
        const s = stateRef.current;
        if (!s.items.length) return;
        const n = s.index + 1 < s.items.length ? s.index + 1 : s.repeat !== 'off' ? 0 : null;
        if (n == null) return;
        loadReq.current = { autoplay: engine.playing, at: 0 };
        dispatch({ type: 'jump', index: n });
      },
      prev() {
        const s = stateRef.current;
        if (!s.items.length) return;
        if (engine.currentTime > 3) {
          engine.seek(0);
          return;
        }
        const p = s.index > 0 ? s.index - 1 : s.repeat === 'all' ? s.items.length - 1 : null;
        if (p == null) {
          engine.seek(0);
          return;
        }
        loadReq.current = { autoplay: engine.playing, at: 0 };
        dispatch({ type: 'jump', index: p });
      },
      remove(index) {
        if (index === stateRef.current.index) loadReq.current = { autoplay: engine.playing, at: 0 };
        dispatch({ type: 'remove', index });
      },
      move(from, to) {
        dispatch({ type: 'move', from, to });
      },
      clearUpcoming() {
        dispatch({ type: 'clearUpcoming' });
      },
      clearAll() {
        dispatch({ type: 'clearAll' });
      },
      toggleShuffle() {
        dispatch({ type: 'toggleShuffle' });
      },
      cycleRepeat() {
        dispatch({ type: 'cycleRepeat' });
      },
      togglePlay() {
        if (!stateRef.current.items.length) return;
        measureRef.current();
        engine.toggle();
      },
      seek(t) {
        engine.seek(t);
      },
      setVolume(v) {
        engine.setVolume(v);
      },
      toggleMute() {
        engine.setMuted(!engine.getSnapshot().muted);
      },
    };
  }, [engine, toast]);

  // 운영체제의 미디어 키, 잠금 화면, 블루투스 이어폰 버튼
  useEffect(() => {
    if (!('mediaSession' in navigator)) return;
    const ms = navigator.mediaSession;
    const set = (a: MediaSessionAction, h: MediaSessionActionHandler | null) => {
      try {
        ms.setActionHandler(a, h);
      } catch {
        // 지원하지 않는 동작
      }
    };
    set('play', () => engine.play());
    set('pause', () => engine.pause());
    set('previoustrack', () => actions.prev());
    set('nexttrack', () => actions.next());
    set('seekto', (d) => d.seekTime != null && engine.seek(d.seekTime));
    set('seekbackward', (d) => engine.seek(engine.currentTime - (d.seekOffset ?? 10)));
    set('seekforward', (d) => engine.seek(engine.currentTime + (d.seekOffset ?? 10)));
    let lastPos = 0;
    const offState = engine.subscribe(() => {
      const s = engine.getSnapshot();
      ms.playbackState = s.status === 'playing' ? 'playing' : s.uid ? 'paused' : 'none';
    });
    const offTime = engine.subscribeTime(() => {
      const now = Date.now();
      if (now - lastPos < 1000) return;
      lastPos = now;
      const duration = engine.getDuration();
      const position = engine.getTime();
      if (duration > 0 && position <= duration) {
        try {
          ms.setPositionState({ duration, position, playbackRate: 1 });
        } catch {
          // 무시
        }
      }
    });
    return () => {
      offState();
      offTime();
    };
  }, [engine, actions]);

  const t = current?.track;
  useEffect(() => {
    if (!('mediaSession' in navigator)) return;
    if (!t) {
      navigator.mediaSession.metadata = null;
      return;
    }
    const abs = (u: string) => new URL(u, location.href).href;
    const artwork = t.cover
      ? [
          { src: abs(t.cover.sm), sizes: '480x480', type: 'image/webp' },
          { src: abs(t.cover.lg), sizes: '1400x1400', type: 'image/webp' },
        ]
      : t.probe
        ? [{ src: `https://archive.org/services/img/${encodeURIComponent(t.probe.id)}` }]
        : [];
    navigator.mediaSession.metadata = new MediaMetadata({ title: t.title, artist: t.artist, album: t.album, artwork });
  }, [t]);

  const queue = useMemo<QueueView>(
    () => ({
      items: state.items,
      index: state.index,
      current,
      shuffle: state.shuffle,
      repeat: state.repeat,
      outputRate,
      canTranscode,
      caps,
      formatOf: (track) => (probes[track.key] ? { ...track.format, ...probes[track.key] } : track.format),
    }),
    [state, current, outputRate, canTranscode, caps, probes],
  );

  const ui = useMemo<Ui>(() => ({ nowPlaying, setNowPlaying, queue: queueOpen, setQueue }), [nowPlaying, queueOpen]);
  const zoneView = useMemo<ZoneView>(() => ({ zone: engine.kind, nativeAvailable }), [engine, nativeAvailable]);

  return (
    <ActionsCtx.Provider value={actions}>
      <QueueCtx.Provider value={queue}>
        <ZoneCtx.Provider value={zoneView}>
          <UiCtx.Provider value={ui}>{children}</UiCtx.Provider>
        </ZoneCtx.Provider>
      </QueueCtx.Provider>
    </ActionsCtx.Provider>
  );
}
