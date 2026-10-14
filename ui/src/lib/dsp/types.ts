/**
 * 소리결 — Sori의 DSP 엔진 설정.
 *
 * 룬의 MUSE를 따라 만들었다. 헤드룸 관리는 체인 맨 앞에 고정이고, 그 뒤로 사용자가 순서를
 * 정하는 필터들, 마지막에 샘플레이트 변환과 비트 깊이 변환이 온다. 모든 계산은 서버의 ffmpeg가
 * 64비트 부동소수점으로 한다 — 룬도 DSP는 기기가 아니라 Core(서버)에서 돌린다.
 *
 * 설정은 기기(존)마다 브라우저에 저장되고, 곡을 요청할 때 필요한 만큼만 서버로 간다.
 */

export type Channel = 'L' | 'R' | 'LR';

export type BandType = 'peak' | 'lowshelf' | 'highshelf' | 'lowpass' | 'highpass' | 'bandpass' | 'notch';

export interface Band {
  type: BandType;
  /** Hz */
  freq: number;
  /** dB. 통과·차단 필터에서는 쓰지 않는다. */
  gain: number;
  q: number;
  on: boolean;
}

export type SrcMode = 'match' | 'max2' | 'compat' | 'custom';
export type SrcFilter = 'precise' | 'smooth';

export interface HeadroomConfig {
  enabled: boolean;
  /** dB, 0 이하 */
  gain: number;
  clipIndicator: boolean;
}

export interface SrcConfig {
  enabled: boolean;
  mode: SrcMode;
  filter: SrcFilter;
  /** SoX 리샘플러의 정밀도(비트). 28 = 매우 높음(VHQ), 33 = 최대. */
  precision: 20 | 28 | 33;
  /** 이 기기에서 쓸 최대 샘플레이트. 2의 거듭제곱·호환성 모드가 본다. */
  maxRate: number;
  /** 사용자 지정: 원본 샘플레이트 → 목표 샘플레이트 */
  custom: Record<string, number>;
}

export type LevelingMode = 'off' | 'auto' | 'track' | 'album';

export interface LevelingConfig {
  mode: LevelingMode;
  /** 목표 라우드니스, LUFS */
  target: number;
  /** 라우드니스를 모르는 곡(분석 전, 라이브 아카이브)에 줄 게인. dB */
  unknown: number;
}

export interface OutputConfig {
  bits: 16 | 24;
  dither: 'tpdf' | 'none';
}

interface FilterBase {
  id: string;
  enabled: boolean;
}

export interface CrossfeedFilter extends FilterBase {
  type: 'crossfeed';
  preset: 'default' | 'cmoy' | 'jmeier' | 'custom';
  /** 사용자 지정일 때만. Hz, 300–2000 */
  fcut: number;
  /** 사용자 지정일 때만. dB, 1–15 */
  feed: number;
}

export interface PeqFilter extends FilterBase {
  type: 'peq';
  preamp: number;
  bands: Band[];
}

export type Step =
  | { op: 'volume'; ch: Channel; gain: number }
  | { op: 'delay'; ch: Channel; ms: number }
  | { op: 'invert'; ch: Channel }
  | { op: 'mute'; ch: Channel }
  | { op: 'mix'; ll: number; lr: number; rl: number; rr: number }
  | { op: 'eq'; ch: Channel; band: Band };

export interface ProceduralFilter extends FilterBase {
  type: 'procedural';
  steps: Step[];
}

export interface HeadphoneFilter extends FilterBase {
  type: 'headphone';
  /** 가져온 프리셋의 이름(예: AutoEQ의 헤드폰 모델). */
  model: string;
  preamp: number;
  bands: Band[];
}

export interface ConvolutionFilter extends FilterBase {
  type: 'convolution';
  /** 서버에 올린 임펄스 응답의 ID */
  ir: string | null;
  irName?: string;
  /** dB */
  gain: number;
  /** 임펄스 응답의 최대 이득을 0dB로 맞춘다 */
  normalize: boolean;
}

export type DspFilter = CrossfeedFilter | PeqFilter | ProceduralFilter | HeadphoneFilter | ConvolutionFilter;
export type FilterType = DspFilter['type'];

export interface DspConfig {
  v: 1;
  /** 이 존의 DSP 엔진 전체 스위치. 볼륨 레벨링은 여기에 묶이지 않는다(룬도 존 설정이다). */
  enabled: boolean;
  headroom: HeadroomConfig;
  src: SrcConfig;
  leveling: LevelingConfig;
  output: OutputConfig;
  /** 모든 필터 우회 */
  bypass: boolean;
  filters: DspFilter[];
}

/**
 * 곡을 요청할 때 서버로 가는 것. 꺼진 것은 빼고, 이 곡에 맞게 정해진 값만 담는다.
 * 서버는 이걸 그대로 믿지 않고 `parseRequest`로 하나하나 검사한다.
 */
export interface DspRequest {
  v: 1;
  /** 헤드룸 dB */
  hr?: number;
  /** 클리핑 표시기 */
  ci?: 1;
  /** 볼륨 레벨링: 기준(트랙/앨범), 목표 LUFS, 모를 때 게인 */
  lv?: { m: 'track' | 'album'; t: number; u: number };
  /** 샘플레이트 변환: 목표 Hz, 필터, 정밀도 */
  src?: { r: number; f: SrcFilter; p: number };
  out: { b: 16 | 24; d: 'tpdf' | 'none' };
  f: WireFilter[];
}

export type WireFilter =
  | { t: 'crossfeed'; p: CrossfeedFilter['preset']; fc?: number; fd?: number }
  | { t: 'peq'; pre: number; b: Band[] }
  | { t: 'procedural'; s: Step[] }
  | { t: 'headphone'; pre: number; b: Band[]; m?: string }
  | { t: 'convolution'; ir: string; g: number; n: boolean; name?: string };

/** 라우드니스. 분석이나 ReplayGain 태그에서 온다. */
export interface LoudnessValue {
  /** LUFS */
  lufs: number;
  /** dBTP(분석) 또는 dBFS(태그) */
  peak: number;
}

export interface Loudness {
  track?: LoudnessValue;
  album?: LoudnessValue;
  source: 'analysis' | 'replaygain';
}
