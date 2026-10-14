/**
 * 라이브러리의 모양.
 *
 * 서버와 브라우저가 같은 타입을 본다. 브라우저로 가는 것은 `Track`까지이고, 파일 경로와
 * 수정 시각처럼 서버만 알아야 하는 것은 `StoredTrack`에만 있다.
 */

/** 음질 등급. Hi-Res는 16비트/48kHz를 넘는 무손실. */
export type QualityTier = 'hires' | 'lossless' | 'lossy';

export interface AudioFormat {
  /** 사람이 읽는 코덱 이름: FLAC, ALAC, WAV, AIFF, MP3, AAC, Vorbis, Opus, DSD … */
  codec: string;
  /** 파일 확장자(소문자). */
  container: string;
  lossless: boolean;
  sampleRate?: number;
  bitDepth?: number;
  /** 초당 비트 수. */
  bitrate?: number;
  channels?: number;
  /** `canPlayType`에 넘길 MIME. 코덱 파라미터까지 붙어 있다. */
  mime: string;
}

export interface CoverRef {
  /** 표지가 바뀌면 같이 바뀌는 값. URL에 붙여 캐시를 깬다. */
  v: string;
  /** 표지의 대표 색. 이미지가 뜨기 전 자리와 배경 틴트에 쓴다. */
  color: string;
}

export interface Track {
  id: string;
  title: string;
  artist: string;
  artistId: string;
  artists: string[];
  album: string;
  albumId: string;
  albumArtist: string;
  albumArtistId: string;
  trackNo?: number;
  discNo?: number;
  year?: number;
  genres: string[];
  /** 초. */
  duration: number;
  format: AudioFormat;
  addedAt: number;
  cover?: CoverRef;
}

/** 디스크에 저장되는 트랙. 다시 스캔할 때 바뀌지 않은 파일을 건너뛰는 근거가 같이 있다. */
export interface StoredTrack extends Omit<Track, 'albumArtist' | 'albumArtistId' | 'artistId' | 'cover'> {
  /** 음악 폴더 기준 상대 경로. 항상 `/`로 구분한다. */
  path: string;
  mtime: number;
  size: number;
  /** 태그에 적힌 앨범 아티스트. 비어 있으면 앨범을 묶을 때 폴더를 본다. */
  albumArtistTag?: string;
}

export interface Album {
  id: string;
  title: string;
  artist: string;
  artistId: string;
  year?: number;
  genres: string[];
  trackIds: string[];
  discCount: number;
  duration: number;
  addedAt: number;
  cover?: CoverRef;
  /** 앨범에서 가장 좋은 트랙의 포맷. 카드와 머리말의 배지가 이걸 쓴다. */
  format: AudioFormat;
}

/** 그리드 카드 하나에 필요한 만큼만. 앨범이 수천 장이어도 가볍게 넘기기 위해서다. */
export type AlbumCard = Pick<Album, 'id' | 'title' | 'artist' | 'artistId' | 'year' | 'addedAt' | 'cover' | 'format'>;

export interface Artist {
  id: string;
  name: string;
  /** 이 사람이 앨범 아티스트인 앨범. */
  albumIds: string[];
  /** 남의 앨범에 참여한 곡. */
  appearsOn: string[];
  /** 대표 이미지로 쓸 앨범. */
  coverAlbumId?: string;
  cover?: CoverRef;
}

export interface Genre {
  id: string;
  name: string;
  albumIds: string[];
  covers: { albumId: string; cover: CoverRef }[];
}

export interface LibrarySummary {
  albums: number;
  artists: number;
  tracks: number;
  /** 초. */
  duration: number;
  bytes: number;
  hires: number;
  lossless: number;
  scannedAt?: number;
}

export interface ScanStatus {
  state: 'idle' | 'scanning' | 'error';
  phase?: 'listing' | 'reading' | 'covers' | 'saving';
  processed: number;
  total: number;
  startedAt?: number;
  finishedAt?: number;
  error?: string;
  added?: number;
  removed?: number;
}

export interface ServerInfo {
  musicDir: string;
  ffmpeg: boolean;
  auth: boolean;
  version: string;
}
