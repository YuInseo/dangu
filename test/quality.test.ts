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

const base = { source: 'library' as const, transcoded: false, dsp: null, volume: 1, muted: false };

test('신호 경로: 원본 그대로면 OS 믹서 앞까지 전부 무손실, 전체는 룬처럼 고품질', () => {
  const p = signalPath({ ...base, format: flacCd, outputRate: 44100 });
  assert.equal(p.quality, 'high');
  assert.ok(p.steps.slice(0, -1).every((s) => s.quality === 'lossless'));
  assert.equal(p.steps.at(-1)!.title, '시스템 출력');
});

test('신호 경로: 출력 장치가 다른 샘플레이트면 브라우저 리샘플링 단계가 생긴다', () => {
  const p = signalPath({ ...base, format: flac2496, outputRate: 48000 });
  const s = p.steps.find((x) => x.title.startsWith('샘플레이트 변환'));
  assert.ok(s);
  assert.equal(s.quality, 'high');
});

test('신호 경로: 볼륨을 줄이면 향상(DSP) 단계', () => {
  const p = signalPath({ ...base, format: flacCd, volume: 0.5, outputRate: 44100 });
  const v = p.steps.find((x) => x.title === '볼륨')!;
  assert.equal(v.quality, 'enhanced');
  assert.match(v.detail, /−6\.0dB/);
});

test('신호 경로: ALAC→FLAC 변환은 샘플이 같아 무손실, DSD→PCM은 고품질', () => {
  const alac = { ...flacCd, codec: 'ALAC', container: 'm4a', mime: 'audio/mp4; codecs="alac"' };
  assert.equal(signalPath({ ...base, format: alac, transcoded: true, outputRate: 44100 }).steps[1]!.quality, 'lossless');
  assert.equal(signalPath({ ...base, format: dsd, transcoded: true, outputRate: 176400 }).steps[1]!.quality, 'high');
});

test('신호 경로: 손실 압축 원본이면 전체가 저품질', () => {
  const p = signalPath({ ...base, format: mp3, outputRate: 44100 });
  assert.equal(p.quality, 'low');
  assert.equal(p.steps[1]!.quality, 'lossless', '전송 자체는 무손실');
});

test('신호 경로: DSP를 켜면 64비트 단계와 FLAC 전송이 끼어든다', () => {
  const p = signalPath({
    ...base,
    format: flac2496,
    outputRate: 48000,
    dsp: { v: 1, hr: -3, src: { r: 48000, f: 'precise', p: 28 }, out: { b: 24, d: 'tpdf' }, f: [] },
  });
  const titles = p.steps.map((s) => s.title);
  assert.deepEqual(titles.slice(0, 7), [
    '원본 · FLAC',
    '비트 깊이 변환',
    '헤드룸 조정',
    '샘플레이트 변환',
    '비트 깊이 변환',
    'Sori 서버',
    'Chrome 디코더'.replace('Chrome', titles[6]!.split(' ')[0]!),
  ]);
  assert.ok(!titles.includes('샘플레이트 변환 · 브라우저'), '서버가 장치에 맞췄으니 브라우저는 리샘플링하지 않는다');
  assert.equal(p.quality, 'high');
});
