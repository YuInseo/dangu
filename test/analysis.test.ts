import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { existsSync, mkdirSync, mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { after, test } from 'node:test';

const hasFfmpeg = spawnSync('ffmpeg', ['-version']).status === 0;
const work = mkdtempSync(path.join(tmpdir(), 'sori-analysis-'));
const music = path.join(work, 'music');
process.env.MUSIC_DIR = music;
process.env.DATA_DIR = path.join(work, 'data');
after(() => rmSync(work, { recursive: true, force: true }));

test('ebur128 요약 읽기', async () => {
  const { parseEbur128 } = await import('../lib/library/analysis');
  const s = `[Parsed_ebur128_1 @ 0x0] Summary:

  Integrated loudness:
    I:          -9.9 LUFS
    Threshold: -20.0 LUFS

  Loudness range:
    LRA:         1.7 LU

  True peak:
    Peak:       -1.0 dBFS`;
  assert.deepEqual(parseEbur128(s), { i: -9.9, p: -1, r: 1.7 });
  assert.deepEqual(parseEbur128(s.replace('-9.9 LUFS', '-inf LUFS').replace('-1.0 dBFS', '-inf dBFS')), { i: -70, p: -120, r: 1.7 });
  assert.equal(parseEbur128('nothing'), null);
});

test('파형: 정해진 개수의 0–255', async () => {
  const { waveform } = await import('../lib/library/analysis');
  const silent = waveform(Buffer.alloc(8000));
  assert.equal(silent.length, 200);
  assert.ok(silent.every((v) => v === 0));
  const loud = Buffer.alloc(8000);
  for (let i = 0; i < 4000; i++) loud.writeInt16LE(i % 2 ? 30000 : -30000, i * 2);
  assert.ok(waveform(loud).every((v) => v > 240));
});

test('분석 → 볼륨 레벨링 값, 분석 전에는 ReplayGain 태그', { skip: !hasFfmpeg && 'ffmpeg 없음' }, async () => {
  mkdirSync(path.join(music, 'A'), { recursive: true });
  const make = (name: string, amp: number, extra: string[] = []) =>
    spawnSync('ffmpeg', [
      '-v',
      'error',
      '-f',
      'lavfi',
      '-i',
      `aevalsrc=${amp}*sin(2*PI*440*t)|${amp}*sin(2*PI*440*t):s=44100:d=4`,
      '-c:a',
      'flac',
      ...extra,
      '-metadata',
      'album=Test',
      '-metadata',
      'artist=T',
      '-metadata',
      `title=${name}`,
      path.join(music, 'A', `${name}.flac`),
    ]);
  make('loud', 0.9);
  make('quiet', 0.05, ['-metadata', 'REPLAYGAIN_TRACK_GAIN=+6.50 dB', '-metadata', 'REPLAYGAIN_TRACK_PEAK=0.050000']);
  assert.ok(existsSync(path.join(music, 'A', 'quiet.flac')));

  const { scanLibrary } = await import('../lib/library/scanner');
  const { buildLibrary } = await import('../lib/library/build');
  const { readAnalysis, runAnalysis } = await import('../lib/library/analysis');
  const { index } = await scanLibrary(null, () => {});

  // 분석 전: 태그가 있는 곡만 값이 있다
  let lib = buildLibrary(index);
  const byTitle = (title: string) => [...lib.views.values()].find((t) => t.title === title)!;
  assert.equal(byTitle('loud').loudness, undefined);
  assert.deepEqual(byTitle('quiet').loudness?.track, { lufs: -24.5, peak: -26 });
  assert.equal(byTitle('quiet').loudness?.source, 'replaygain');

  const analysis = await readAnalysis();
  let batches = 0;
  await runAnalysis({ root: index.root, tracks: index.tracks, analysis, speed: 'fast', onProgress: () => {}, onBatch: () => batches++ })
    .done;
  assert.ok(batches >= 1);
  lib = buildLibrary(index, analysis);
  const loud = byTitle('loud').loudness!;
  assert.equal(loud.source, 'analysis');
  // 0.9 진폭 사인파가 양쪽 채널에: 채널 에너지를 더하므로 -0.691 + 10·log10(2·0.9²/2) ≈ -1.6 LUFS
  assert.ok(Math.abs(loud.track!.lufs - -1.6) < 0.3, `lufs ${loud.track!.lufs}`);
  assert.ok(loud.track!.peak > -1.5 && loud.track!.peak < 0, `peak ${loud.track!.peak}`);
  assert.ok(loud.album, '앨범 라우드니스도');
  assert.ok(loud.album!.lufs > byTitle('quiet').loudness!.track!.lufs);
  // 다시 돌리면 할 일이 없다
  const { pendingTracks } = await import('../lib/library/analysis');
  assert.equal(pendingTracks(index.tracks, analysis).length, 0);
});
