import assert from 'node:assert/strict';
import { readFileSync, existsSync } from 'node:fs';
import { test } from 'node:test';
import { parseFlacStreamInfo } from '../src/lib/live/flac';

const sample = 'music/Sori Samples/Neon Harbor/2023 - Night Drive/01 Harbor Lights.flac';

test('STREAMINFO: 24비트 96kHz', { skip: !existsSync(sample) && '시험용 라이브러리가 없습니다 (npm run sample)' }, () => {
  const info = parseFlacStreamInfo(readFileSync(sample).subarray(0, 65536));
  assert.ok(info);
  assert.equal(info.sampleRate, 96000);
  assert.equal(info.bitDepth, 24);
  assert.equal(info.channels, 2);
});

test('STREAMINFO: 손으로 만든 머리 + 앞에 붙은 ID3', () => {
  // fLaC, STREAMINFO 블록(마지막 아님, 길이 34), 44100Hz 2채널 16비트, 총 샘플 441000
  const si = new Uint8Array(34);
  si.set([0x10, 0x00, 0x10, 0x00], 0); // min/max block
  // 샘플레이트 20비트 = 44100 = 0x0AC44 → 0x0A 0xC4 0x4_, 채널-1=1, 비트-1=15
  si[10] = 0x0a;
  si[11] = 0xc4;
  si[12] = (0x4 << 4) | (1 << 1) | (15 >> 4);
  si[13] = ((15 & 0x0f) << 4) | 0x0;
  const total = 441000;
  si[14] = (total >>> 24) & 0xff;
  si[15] = (total >>> 16) & 0xff;
  si[16] = (total >>> 8) & 0xff;
  si[17] = total & 0xff;
  const flac = new Uint8Array([0x66, 0x4c, 0x61, 0x43, 0x00, 0x00, 0x00, 34, ...si]);
  const id3 = new Uint8Array([0x49, 0x44, 0x33, 3, 0, 0, 0, 0, 0, 5, 1, 2, 3, 4, 5]);
  for (const buf of [flac, new Uint8Array([...id3, ...flac])]) {
    const info = parseFlacStreamInfo(buf);
    assert.deepEqual(info, { sampleRate: 44100, channels: 2, bitDepth: 16, totalSamples: 441000 });
  }
  assert.equal(parseFlacStreamInfo(new Uint8Array([1, 2, 3])), null);
});
