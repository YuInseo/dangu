/**
 * 라이브 아카이브에서 서버와 브라우저가 같이 쓰는 것. 네트워크 코드는 `archive.ts`에.
 */
import type { AudioFormat } from '../library/types';

export interface LiveShow {
  id: string;
  title: string;
  band: string;
  date?: string;
  venue?: string;
  city?: string;
  downloads?: number;
  /** FLAC 원본이 있는지, 24비트인지. 검색 결과의 format 목록에서 판단한다. */
  quality: 'flac24' | 'flac' | 'lossy' | 'unknown';
}

export interface LiveTrack {
  file: string;
  title: string;
  trackNo?: number;
  duration: number;
  format: AudioFormat;
  url: string;
  /** 파일 크기(바이트) */
  size?: number;
}

export interface LiveShowDetail extends LiveShow {
  notes?: string;
  source?: string;
  lineage?: string;
  taper?: string;
  tracks: LiveTrack[];
  /** 라이브러리에 저장(내려받기)해도 되는가. 아티스트가 듣기만 허락한 공연은 아니다. */
  saveable: boolean;
  /** 저장할 수 없으면 그 까닭 */
  saveBlock?: string;
}

export type LiveSort = 'popular' | 'recent' | 'newest';

export const LIVE_SORT_LABEL: Record<LiveSort, string> = {
  popular: '인기',
  recent: '최근 올라온',
  newest: '최근 공연',
};

export const FEATURED_BANDS = [
  'King Gizzard & The Lizard Wizard',
  'Grateful Dead',
  'Billy Strings',
  'Goose',
  "Umphrey's McGee",
  'The Smashing Pumpkins',
  'moe.',
  'Railroad Earth',
];

/** "1977-05-08" → "1977. 5. 8." */
export function prettyDate(d?: string): string {
  if (!d) return '';
  const m = /^(\d{4})-(\d{2})-(\d{2})/.exec(d);
  return m ? `${m[1]}. ${Number(m[2])}. ${Number(m[3])}.` : d;
}

export const LIVE_QUALITY_LABEL: Record<LiveShow['quality'], string | null> = {
  flac24: 'FLAC 24-bit',
  flac: 'FLAC',
  lossy: 'MP3',
  unknown: null,
};
