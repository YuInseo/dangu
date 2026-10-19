'use client';

import { useEffect, useImperativeHandle, useRef, useState, type Ref } from 'react';
import { CircleAlert, ExternalLink, RotateCcw } from '@/components/icons';
import { useEngineState, usePlayer } from '@/lib/player/context';
import { EMBED_OFF, playerFailure, type YtFailure } from '@/lib/youtube';

/** 유튜브 IFrame 플레이어 API(공식). 필요한 만큼만 적어 둔다. */
interface YtApiPlayer {
  destroy(): void;
  pauseVideo(): void;
  playVideo(): void;
  seekTo(sec: number, allowSeekAhead: boolean): void;
  getPlayerState(): number;
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
  ref,
}: {
  id: string;
  list?: string;
  start?: number;
  blocked?: boolean;
  thumb?: string;
  ref?: Ref<YtPlayerHandle>;
}) {
  const host = useRef<HTMLDivElement>(null);
  const player = useRef<YtApiPlayer | null>(null);
  const { engine } = usePlayer();
  const sori = useEngineState();
  const [failed, setFailed] = useState<YtFailure | null>(null);
  const [attempt, setAttempt] = useState(0);

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
              if (e.data === YT.PlayerState.PLAYING && engine.getSnapshot().status === 'playing') engine.pause();
              markPlaying(live && (e.data === YT.PlayerState.PLAYING || e.data === YT.PlayerState.BUFFERING));
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

  // 소리를 다시 틀면 영상은 멈춘다
  useEffect(() => {
    if (sori.status !== 'playing') return;
    try {
      if (player.current?.getPlayerState() === 1) player.current.pauseVideo();
    } catch {
      // 아직 준비 전
    }
  }, [sori.status]);

  // 휴대폰에서 앱을 떠나거나 화면이 꺼지면 영상도 멈춘다. 유튜브 정책이 다른 앱의 백그라운드 재생을 막는다
  // (휴대폰의 유튜브에서는 프리미엄의 기능). 컴퓨터는 유튜브도 창을 내려 두면 계속 튼다. 소리의 음악은
  // 어디서나 뒤에서 계속 튼다.
  useEffect(() => {
    if (!/Android|iPhone|iPad/i.test(navigator.userAgent)) return;
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
  }, []);

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
