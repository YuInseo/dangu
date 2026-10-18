'use client';

import { useEffect, useImperativeHandle, useRef, useState, type Ref } from 'react';
import { useEngineState, usePlayer } from '@/lib/player/context';

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
  PlayerState: { PLAYING: number; ENDED: number };
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
      reject(new Error('유튜브 플레이어를 불러오지 못했습니다. 인터넷에 닿는지 확인해 주세요'));
    };
    document.head.appendChild(s);
  });
  return loading;
}

/** 플레이어 오류 번호 → 사람이 읽는 말 */
function playerError(code: number): string {
  // 101·150: 올린 사람이 다른 곳에서 틀지 못하게 막았거나, 유튜브가 로그인을 요구한다("봇이 아닌지 확인").
  if (code === 101 || code === 150)
    return `유튜브가 이 영상을 앱 안에서 틀지 못하게 했습니다(오류 ${code}). 올린 사람이 막아 두었거나 유튜브가 로그인을 요구하는 경우입니다.`;
  if (code === 153) return '유튜브 플레이어가 이 앱의 주소를 확인하지 못했습니다(오류 153).';
  if (code === 100) return '영상을 찾을 수 없습니다(지워졌거나 비공개).';
  if (code === 2) return '영상 주소가 이상합니다.';
  return `이 기기에서 틀 수 없는 영상입니다(오류 ${code}).`;
}

export interface YtPlayerHandle {
  seek(sec: number): void;
}

/**
 * 유튜브 공식 플레이어. 영상을 틀면 소리의 재생은 멈추고, 소리를 다시 틀면 영상이 멈춘다(소리 둘이
 * 겹치지 않게). 광고·자막·화질·전체 화면은 유튜브 플레이어가 그대로 한다.
 */
export function YtPlayer({ id, list, start, ref }: { id: string; list?: string; start?: number; ref?: Ref<YtPlayerHandle> }) {
  const host = useRef<HTMLDivElement>(null);
  const player = useRef<YtApiPlayer | null>(null);
  const { engine } = usePlayer();
  const sori = useEngineState();
  const [failed, setFailed] = useState<string | null>(null);

  useEffect(() => {
    let live = true;
    setFailed(null);
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
            },
            onError: (e: { data: number }) => setFailed(playerError(e.data)),
          },
        });
      })
      .catch((e: unknown) => live && setFailed(e instanceof Error ? e.message : '유튜브 플레이어를 불러오지 못했습니다'));
    return () => {
      live = false;
      try {
        player.current?.destroy();
      } catch {
        // 이미 사라졌다
      }
      player.current = null;
    };
    // 시작 위치는 처음 열 때만 쓴다
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [id, list, engine]);

  // 소리를 다시 틀면 영상은 멈춘다
  useEffect(() => {
    if (sori.status !== 'playing') return;
    try {
      if (player.current?.getPlayerState() === 1) player.current.pauseVideo();
    } catch {
      // 아직 준비 전
    }
  }, [sori.status]);

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

  // 틀 수 없을 때 유튜브 플레이어도 그 자리에 까닭을 보이므로, 우리 안내는 덮지 않고 그 아래에 둔다.
  return (
    <>
      <div className="yt-player">
        <div ref={host} className="yt-player-frame" />
      </div>
      {failed && (
        <div className="yt-player-note" role="alert">
          <p>{failed}</p>
          <a className="btn btn-sm" href={`https://www.youtube.com/watch?v=${id}`} target="_blank" rel="noreferrer">
            유튜브에서 보기
          </a>
        </div>
      )}
    </>
  );
}
