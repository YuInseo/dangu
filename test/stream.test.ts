import assert from 'node:assert/strict';
import { existsSync, mkdtempSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { after, before, test } from 'node:test';
import { NextRequest } from 'next/server';

const music = path.resolve('music');
const skip = !existsSync(path.join(music, 'Sori Samples')) && '시험용 라이브러리가 없습니다 (npm run sample)';
const data = mkdtempSync(path.join(tmpdir(), 'sori-stream-'));
process.env.MUSIC_DIR = music;
process.env.DATA_DIR = data;

type Route = typeof import('../app/api/stream/[id]/route');
let GET: Route['GET'];
let ids: Map<string, string>;

before(async () => {
  if (skip) return;
  const store = await import('../lib/library/store');
  await store.getLibrary();
  await store.waitForScan();
  const lib = await store.getLibrary();
  ids = new Map([...lib.tracks.values()].map((t) => [t.path, t.id]));
  ({ GET } = await import('../app/api/stream/[id]/route'));
});

after(() => rmSync(data, { recursive: true, force: true }));

const call = (id: string, headers: Record<string, string> = {}, query = '') =>
  GET(new NextRequest(`http://localhost/api/stream/${id}${query}`, { headers }), { params: Promise.resolve({ id }) });

const FILES = [
  'Sori Samples/Neon Harbor/2023 - Night Drive/01 Harbor Lights.flac',
  'Sori Samples/Mira Sol/Glass Garden/01 Greenhouse.m4a',
  'Sori Samples/Cassette Club/Lo-Fi Sketches/01 Tape Hiss Morning.mp3',
  'Sori Samples/하늘 사중주/아침의 차/01 찻잔 위의 김.flac',
];

test('보내는 바이트가 파일과 똑같다', { skip }, async () => {
  for (const rel of FILES) {
    const res = await call(ids.get(rel)!);
    assert.equal(res.status, 200, rel);
    const body = Buffer.from(await res.arrayBuffer());
    assert.ok(body.equals(readFileSync(path.join(music, rel))), rel);
  }
});

test('범위 요청', { skip }, async () => {
  const rel = FILES[0]!;
  const orig = readFileSync(path.join(music, rel));
  const id = ids.get(rel)!;

  let res = await call(id, { range: 'bytes=1000-1999' });
  assert.equal(res.status, 206);
  assert.equal(res.headers.get('content-range'), `bytes 1000-1999/${orig.length}`);
  assert.ok(Buffer.from(await res.arrayBuffer()).equals(orig.subarray(1000, 2000)));

  res = await call(id, { range: 'bytes=-500' });
  assert.ok(Buffer.from(await res.arrayBuffer()).equals(orig.subarray(-500)));

  res = await call(id, { range: `bytes=${orig.length + 10}-` });
  assert.equal(res.status, 416);
});

test('없는 곡은 404', { skip }, async () => {
  assert.equal((await call('nope')).status, 404);
});

test('ALAC을 FLAC으로 바꿔도 샘플은 그대로', { skip }, async () => {
  const { spawnSync } = await import('node:child_process');
  if (spawnSync('ffmpeg', ['-version']).status !== 0) return;
  const rel = FILES[1]!;
  const res = await call(ids.get(rel)!, {}, '?transcode=1');
  assert.equal(res.headers.get('content-type'), 'audio/flac');
  const flac = Buffer.from(await res.arrayBuffer());
  const pcm = (input: string | Buffer) =>
    spawnSync('ffmpeg', ['-v', 'error', '-i', typeof input === 'string' ? input : 'pipe:0', '-map', '0:a', '-f', 's32le', '-'], {
      input: typeof input === 'string' ? undefined : input,
      maxBuffer: 1 << 30,
    }).stdout;
  assert.ok(pcm(flac).equals(pcm(path.join(music, rel))));
});
