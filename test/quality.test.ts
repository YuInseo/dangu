import assert from 'node:assert/strict';
import { test } from 'node:test';
import { betterFormat, formatLabel, khz, tierOf } from '../lib/quality';
import { signalPath } from '../lib/player/signal';
import type { AudioFormat } from '../lib/library/types';

const flac2496: AudioFormat = {
  codec: 'FLAC',
  container: 'flac',
  lossless: true,
  bitDepth: 24,
  sampleRate: 96000,
  channels: 2,
  mime: 'audio/flac',
};
const flacCd: AudioFormat = {
  codec: 'FLAC',
  container: 'flac',
  lossless: true,
  bitDepth: 16,
  sampleRate: 44100,
  channels: 2,
  mime: 'audio/flac',
};
const mp3: AudioFormat = {
  codec: 'MP3',
  container: 'mp3',
  lossless: false,
  sampleRate: 44100,
  bitrate: 320000,
  channels: 2,
  mime: 'audio/mpeg',
};
const dsd: AudioFormat = { codec: 'DSD', container: 'dsf', lossless: true, sampleRate: 2822400, channels: 2, mime: 'audio/x-dsf' };

test('등급', () => {
  assert.equal(tierOf(flac2496), 'hires');
  assert.equal(tierOf(flacCd), 'lossless');
  assert.equal(tierOf(mp3), 'lossy');
  assert.equal(tierOf({ ...flacCd, bitDepth: 24, sampleRate: 48000 }), 'hires');
});

test('짧은 이름', () => {
  assert.equal(khz(44100), '44.1');
  assert.equal(khz(96000), '96');
  assert.equal(formatLabel(flac2496), 'FLAC 24/96');
  assert.equal(formatLabel(flacCd), 'FLAC 16/44.1');
  assert.equal(formatLabel(mp3), 'MP3 320k');
  assert.equal(formatLabel(dsd), 'DSD64');
});

test('더 좋은 포맷', () => {
  assert.equal(betterFormat(flacCd, flac2496), flac2496);
  assert.equal(betterFormat(flac2496, mp3), flac2496);
  assert.equal(betterFormat(mp3, flacCd), flacCd);
});

test('신호 경로: 원본 그대로, 볼륨 100%, 샘플레이트 일치면 무손실', () => {
  const p = signalPath({ format: flacCd, source: 'library', transcoded: false, volume: 1, muted: false, outputRate: 44100 });
  assert.equal(p.quality, 'lossless');
});

test('신호 경로: 출력 장치가 다른 샘플레이트면 고품질로 내려간다', () => {
  const p = signalPath({ format: flac2496, source: 'library', transcoded: false, volume: 1, muted: false, outputRate: 48000 });
  assert.equal(p.quality, 'high');
  assert.match(p.steps.at(-1)!.detail, /리샘플링/);
});

test('신호 경로: 볼륨을 줄이면 DSP', () => {
  const p = signalPath({ format: flacCd, source: 'library', transcoded: false, volume: 0.5, muted: false, outputRate: 44100 });
  assert.equal(p.quality, 'enhanced');
});

test('신호 경로: ALAC→FLAC 변환은 샘플이 같아 무손실, DSD→PCM은 고품질', () => {
  const alac = { ...flacCd, codec: 'ALAC', container: 'm4a', mime: 'audio/mp4; codecs="alac"' };
  assert.equal(
    signalPath({ format: alac, source: 'library', transcoded: true, volume: 1, muted: false, outputRate: 44100 }).quality,
    'lossless',
  );
  assert.equal(
    signalPath({ format: dsd, source: 'library', transcoded: true, volume: 1, muted: false, outputRate: 176400 }).quality,
    'high',
  );
});

test('신호 경로: 손실 압축은 끝까지 손실', () => {
  assert.equal(
    signalPath({ format: mp3, source: 'library', transcoded: false, volume: 1, muted: false, outputRate: 44100 }).quality,
    'low',
  );
});
