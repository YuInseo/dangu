import type { AudioFormat } from '../library/types';

/** 재생할 수 있는 것 하나. 내 라이브러리의 곡이든 라이브 아카이브의 곡이든 같은 모양이다. */
export interface PlayableTrack {
  /** `lib:<id>` 또는 `live:<공연>/<파일>`. 같은 곡이면 같은 값. */
  key: string;
  source: 'library' | 'live';
  title: string;
  artist: string;
  album: string;
  albumHref?: string;
  artistHref?: string;
  albumId?: string;
  year?: number;
  trackNo?: number;
  discNo?: number;
  duration: number;
  format: AudioFormat;
  /** 원본 그대로 받는 주소. */
  src: string;
  /** 브라우저가 못 여는 형식일 때 서버가 FLAC으로 바꿔 보내는 주소. */
  transcodeSrc?: string;
  cover?: { sm: string; lg: string; color?: string };
  /** 표지가 없을 때 대신 그릴 그림의 재료. */
  art: { seed: string; title: string; subtitle?: string };
  /** 라이브 FLAC은 실제 샘플레이트를 따로 물어봐야 안다. */
  probe?: { id: string; file: string };
}

/** 대기열의 한 칸. 같은 곡을 두 번 넣어도 칸은 다르다. */
export interface QueueItem {
  uid: string;
  track: PlayableTrack;
}

export type RepeatMode = 'off' | 'all' | 'one';
