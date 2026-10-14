/**
 * 파일 확장자와 태그 파서의 코덱 문자열을 우리 `AudioFormat`으로.
 */
import type { IFormat } from 'music-metadata';
import type { AudioFormat } from './types';

export const AUDIO_EXTENSIONS = new Set([
  'flac',
  'mp3',
  'm4a',
  'mp4',
  'aac',
  'wav',
  'aif',
  'aiff',
  'aifc',
  'ogg',
  'oga',
  'opus',
  'wv',
  'ape',
  'dsf',
  'dff',
  'wma',
]);

const LOSSLESS_CODECS = new Set(['FLAC', 'ALAC', 'WAV', 'AIFF', 'PCM', 'WavPack', 'APE', 'DSD']);

export function codecName(raw: string | undefined, ext: string): string {
  const c = (raw ?? '').toLowerCase();
  if (c.includes('flac')) return 'FLAC';
  if (c.includes('alac')) return 'ALAC';
  if (c.includes('layer 3') || (!c && ext === 'mp3')) return 'MP3';
  if (c.includes('aac') || c.includes('mp4a')) return 'AAC';
  if (c.includes('vorbis')) return 'Vorbis';
  if (c.includes('opus')) return 'Opus';
  if (c.includes('wavpack') || ext === 'wv') return 'WavPack';
  if (c.includes('monkey') || ext === 'ape') return 'APE';
  if (c.includes('dsd') || ext === 'dsf' || ext === 'dff') return 'DSD';
  if (c.includes('wma') || c.includes('windows media') || ext === 'wma') return 'WMA';
  if (c.includes('pcm') || c.includes('float') || c.includes('lpcm') || c.includes('twos') || c.includes('sowt')) {
    if (ext === 'wav') return 'WAV';
    if (ext.startsWith('aif')) return 'AIFF';
    return 'PCM';
  }
  if (ext === 'flac') return 'FLAC';
  if (ext === 'wav') return 'WAV';
  if (ext.startsWith('aif')) return 'AIFF';
  return raw || ext.toUpperCase();
}

/** 브라우저에 `canPlayType`으로 물어볼 MIME. */
export function mimeFor(codec: string, ext: string): string {
  switch (codec) {
    case 'FLAC':
      return ext === 'ogg' || ext === 'oga' ? 'audio/ogg; codecs="flac"' : 'audio/flac';
    case 'MP3':
      return 'audio/mpeg';
    case 'AAC':
      return ext === 'aac' ? 'audio/aac' : 'audio/mp4; codecs="mp4a.40.2"';
    case 'ALAC':
      return 'audio/mp4; codecs="alac"';
    case 'WAV':
      return 'audio/wav';
    case 'AIFF':
      return 'audio/aiff';
    case 'Vorbis':
      return 'audio/ogg; codecs="vorbis"';
    case 'Opus':
      return ext === 'webm' ? 'audio/webm; codecs="opus"' : 'audio/ogg; codecs="opus"';
    case 'WavPack':
      return 'audio/x-wavpack';
    case 'APE':
      return 'audio/x-ape';
    case 'DSD':
      return 'audio/x-dsf';
    case 'WMA':
      return 'audio/x-ms-wma';
    default:
      return 'application/octet-stream';
  }
}

export function toAudioFormat(f: IFormat, ext: string): AudioFormat {
  const codec = codecName(f.codec, ext);
  const lossless = f.lossless ?? LOSSLESS_CODECS.has(codec);
  return {
    codec,
    container: ext,
    lossless: lossless || LOSSLESS_CODECS.has(codec),
    sampleRate: f.sampleRate || undefined,
    // DSD의 1비트는 PCM 비트 깊이와 비교할 수 없어 적지 않는다.
    bitDepth: codec === 'DSD' ? undefined : f.bitsPerSample || undefined,
    bitrate: f.bitrate ? Math.round(f.bitrate) : undefined,
    channels: f.numberOfChannels || undefined,
    mime: mimeFor(codec, ext),
  };
}
