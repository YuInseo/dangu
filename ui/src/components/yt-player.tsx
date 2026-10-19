'use client';

import { useCallback, useEffect, useImperativeHandle, useRef, useState, type Ref } from 'react';
import { CircleAlert, ExternalLink, RotateCcw } from '@/components/icons';
import { useLibraryInfo } from '@/library';
import { isLocalOrigin } from '@/lib/link';
import { useEngineState, usePlayer } from '@/lib/player/context';
import { usePrefs } from '@/lib/prefs';
import { EMBED_OFF, playerFailure, type YtFailure } from '@/lib/youtube';

/** 유튜브 IFrame 플레이어 API(공식). 필요한 만큼만 적어 둔다. */
interface YtApiPlayer {
  destroy(): void;
  pauseVideo(): void;
  playVideo(): void;
  seekTo(sec: number, allowSeekAhead: boolean): void;
  getPlayerState(): number;
  nextVideo?(): void;
  previousVideo?(): void;
}

interface YtApi {
  Player: new (el: HTMLElement, opts: Record<string, unknown>) => YtApiPlayer;
  PlayerState: { PLAYING: number; ENDED: number; BUFFERING: number };
}

declare global {
  interface Window {
    YT?: YtApi;
    onYouTubeIframeAPIReady?: () => void;
  }
}

let loading: Promise<YtApi> | null = null;

function loadApi(): Promise<YtApi> {
  if (window.YT?.Player) return Promise.resolve(window.YT);
  loading ??= new Promise<YtApi>((resolve, reject) => {
    const before = window.onYouTubeIframeAPIReady;
    window.onYouTubeIframeAPIReady = () => {
      before?.();
      if (window.YT) resolve(window.YT);
    };
    const s = document.createElement('script');
    s.src = 'https://www.youtube.com/iframe_api';
    s.async = true;
    s.onerror = () => {
      loading = null;
      reject(new Error('load'));
    };
    document.head.appendChild(s);
  });
  return loading;
}

const NO_API: YtFailure = { title: '유튜브 플레이어를 불러오지 못했습니다', detail: '인터넷에 닿는지 확인해 주세요.' };

/** 영상이 도는 동안: 휴대폰에서는 아래쪽 소리 재생 막대를 내려 둔다(CSS가 이 표시를 본다). */
function markPlaying(on: boolean) {
  document.documentElement.classList.toggle('yt-playing', on);
}

export interface YtPlayerHandle {
  seek(sec: number): void;
}

/**
 * 유튜브 공식 플레이어. 영상을 틀면 소리의 재생은 멈추고, 소리를 다시 틀면 영상이 멈춘다(소리 둘이
 * 겹치지 않게). 광고·자막·화질·전체 화면은 유튜브 플레이어가 그대로 한다.
 *
 * `blocked`: 영상 정보에서 이미 유튜브 밖 재생이 꺼진 것을 안다(플레이어를 띄우지 않는다).
 */
export function YtPlayer({
  id,
  list,
  start,
  blocked,
  thumb,
  title,
  channel,
  ref,
}: {
  id: string;
  list?: string;
  start?: number;
  blocked?: boolean;
  thumb?: string;
  /** 알림·잠금 화면에 보일 제목과 채널 */
  title?: string;
  channel?: string;
  ref?: Ref<YtPlayerHandle>;
}) {
  const host = useRef<HTMLDivElement>(null);
  const player = useRef<YtApiPlayer | null>(null);
  const { engine } = usePlayer();
  const sori = useEngineState();
  const [failed, setFailed] = useState<YtFailure | null>(null);
  const [attempt, setAttempt] = useState(0);
  // 영상이 소리(엔진)를 멈춘 때
  const tookOver = useRef(-Infinity);
  // 휴대폰 앱 안에서 백그라운드 재생을 켜 두었으면: 지금 트는 영상을 소리(서버)에 알려 알림·잠금 화면이
  // 보이게 하고, 그 단추(재생·일시정지)를 따른다. 앱이 뒤로 가도 알림의 전경 서비스가 앱을 살려 둔다.
  const background = usePrefs().ytBackground;
  const inApp = useLibraryInfo().host === 'android' && isLocalOrigin();
  const share = inApp && background;
  // 이 화면의 표: 서버는 이 표로 연 상태 흐름이 닫히면(화면이 사라지면) 알림에서 영상을 내린다
  const [owner] = useState(() => `yt-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 10)}`);
  const now = useRef({ title, channel, thumb, share, playing: false, told: false });
  const tell = useCallback(
    (playing: boolean) => {
      const n = now.current;
      n.playing = playing;
      // 알림에 없는 영상이 멈춘 것은 알리지 않는다(소리를 다시 틀어 알림을 넘겨준 뒤처럼)
      if (!n.share || (!playing && !n.told)) return;
      n.told = true;
      void fetch('/api/player/external', {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify({ title: n.title ?? '', artist: n.channel ?? '', cover: n.thumb, playing, owner }),
      }).catch(() => {});
    },
    [owner],
  );
  // 알림을 엔진에 돌려준다
  const forget = useCallback(() => {
    if (!now.current.told) return;
    now.current.told = false;
    void fetch(`/api/player/external?owner=${owner}`, { method: 'DELETE', keepalive: true }).catch(() => {});
  }, [owner]);
  useEffect(() => {
    const n = now.current;
    const changed = n.title !== title || n.channel !== channel || n.thumb !== thumb;
    Object.assign(n, { title, channel, thumb, share });
    // 영상 정보가 나중에 오면(재생이 먼저 시작됐으면) 알림도 고친다
    if (changed && n.told) tell(n.playing);
  }, [title, channel, thumb, share, tell]);
  useEffect(() => {
    if (!share) return;
    const es = new EventSource(`/api/player/events?owner=${owner}`);
    let seen: number | null = null;
    // 붙으면 지금 상태를 알린다: 다시 붙었으면(끊긴 사이 서버가 내렸다), 또는 영상이 도는 중에 설정을 켰으면
    es.addEventListener('open', () => {
      const n = now.current;
      if (n.told || n.playing) tell(n.playing);
    });
    es.addEventListener('player', (ev) => {
      try {
        const ext = (JSON.parse((ev as MessageEvent<string>).data) as { external?: { cmd?: string; cmdSeq: number } }).external;
        if (!ext) return;
        // 처음 받은 번호까지는 지난 명령이다
        if (seen == null || ext.cmdSeq <= seen) {
          seen = Math.max(seen ?? 0, ext.cmdSeq);
          return;
        }
        seen = ext.cmdSeq;
        const p = player.current;
        if (!p) return;
        if (ext.cmd === 'play') p.playVideo();
        else if (ext.cmd === 'pause') p.pauseVideo();
        else if (ext.cmd === 'next') p.nextVideo?.();
        else if (ext.cmd === 'prev') p.previousVideo?.();
      } catch {
        // 아직 준비 전
      }
    });
    // 영상 화면을 떠나면(또는 화면이 새로 읽히면) 알림은 엔진으로 돌아간다
    window.addEventListener('pagehide', forget);
    return () => {
      window.removeEventListener('pagehide', forget);
      es.close();
      forget();
    };
  }, [share, owner, tell, forget]);

  useEffect(() => {
    if (blocked) return;
    let live = true;
    setFailed(null);
    const drop = () => {
      try {
        player.current?.destroy();
      } catch {
        // 이미 사라졌다
      }
      player.current = null;
    };
    loadApi()
      .then((YT) => {
        if (!live || !host.current) return;
        const el = document.createElement('div');
        host.current.replaceChildren(el);
        player.current = new YT.Player(el, {
          host: 'https://www.youtube-nocookie.com',
          videoId: id,
          width: '100%',
          height: '100%',
          playerVars: {
            autoplay: 1,
            playsinline: 1,
            rel: 0,
            origin: window.location.origin,
            ...(start ? { start: Math.floor(start) } : {}),
            ...(list ? { list, listType: 'playlist' } : {}),
          },
          events: {
            onStateChange: (e: { data: number }) => {
              if (e.data === YT.PlayerState.PLAYING && engine.getSnapshot().status === 'playing') {
                tookOver.current = performance.now();
                engine.pause();
              }
              const on = e.data === YT.PlayerState.PLAYING || e.data === YT.PlayerState.BUFFERING;
              markPlaying(live && on);
              // 버퍼링은 재생 중으로 친다. 처음 불러오는 것(-1, 5)은 알리지 않는다.
              if (live && (on || e.data === 2 || e.data === YT.PlayerState.ENDED)) tell(on);
            },
            // 틀 수 없는 플레이어는 걷어 내고 그 자리에 까닭을 보인다(플레이어 위를 덮지 않는다).
            onError: (e: { data: number }) => {
              if (!live) return;
              markPlaying(false);
              setFailed(playerFailure(e.data));
              setTimeout(drop, 0);
            },
          },
        });
      })
      .catch(() => live && setFailed(NO_API));
    return () => {
      live = false;
      markPlaying(false);
      drop();
    };
    // 시작 위치는 처음 열 때만 쓴다
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [id, list, engine, blocked, attempt]);

  // 소리를 다시 틀면 영상은 멈추고, 알림도 소리에 돌려준다. 영상이 소리를 멈춘 직후에 늦게 온 "재생 중"은
  // 지난 상태다(그것으로 영상을 멈추지 않는다).
  useEffect(() => {
    if (sori.status !== 'playing' || performance.now() - tookOver.current < 1500) return;
    forget();
    try {
      if (player.current?.getPlayerState() === 1) player.current.pauseVideo();
    } catch {
      // 아직 준비 전
    }
  }, [sori.status, forget]);

  // 백그라운드 재생을 꺼 두었으면: 휴대폰에서 앱을 떠나거나 화면이 꺼질 때 영상도 멈춘다(유튜브 정책은 다른
  // 앱의 백그라운드 재생을 막는다. 휴대폰 유튜브에서는 프리미엄 기능). 컴퓨터는 창을 내려도 계속 튼다.
  useEffect(() => {
    if (background || !/Android|iPhone|iPad/i.test(navigator.userAgent)) return;
    const onHide = () => {
      if (document.visibilityState !== 'hidden') return;
      try {
        const s = player.current?.getPlayerState();
        if (s === 1 || s === 3) player.current?.pauseVideo();
      } catch {
        // 아직 준비 전
      }
    };
    document.addEventListener('visibilitychange', onHide);
    return () => document.removeEventListener('visibilitychange', onHide);
  }, [background]);

  useImperativeHandle(
    ref,
    () => ({
      seek(sec: number) {
        try {
          player.current?.seekTo(sec, true);
          player.current?.playVideo();
          host.current?.scrollIntoView({ block: 'nearest', behavior: 'smooth' });
        } catch {
          // 아직 준비 전
        }
      },
    }),
    [],
  );

  const fail = blocked ? EMBED_OFF : failed;
  return (
    <div className={`yt-player${fail ? ' is-failed' : ''}`}>
      <div ref={host} className="yt-player-frame" hidden={!!fail} />
      {fail && (
        <div className="yt-player-fail" role="alert">
          <img
            src={thumb ?? `https://i.ytimg.com/vi/${id}/hqdefault.jpg`}
            alt=""
            referrerPolicy="no-referrer"
            onError={(e) => (e.currentTarget.hidden = true)}
          />
          <div className="yt-player-fail-body">
            <CircleAlert size={22} className="yt-player-fail-icon" />
            <strong>{fail.title}</strong>
            {fail.detail && <p>{fail.detail}</p>}
            {fail.code != null && <span className="yt-player-fail-code">오류 {fail.code}</span>}
            <div className="yt-player-fail-actions">
              {fail === NO_API && (
                <button type="button" className="btn btn-sm" onClick={() => setAttempt((n) => n + 1)}>
                  <RotateCcw size={15} /> 다시 시도
                </button>
              )}
              <a className="btn btn-sm yt-player-fail-btn" href={`https://www.youtube.com/watch?v=${id}`} target="_blank" rel="noreferrer">
                <ExternalLink size={15} /> 유튜브에서 보기
              </a>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
