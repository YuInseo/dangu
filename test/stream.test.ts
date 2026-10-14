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

test('DSP: 64비트 체인을 지나 24비트/48kHz FLAC으로 나오고, 클리핑이 난 곳을 기록한다', { skip }, async () => {
  const { encodeRequest } = await import('../lib/dsp/request');
  const { getClips } = await import('../lib/dsp/clips');
  const { spawnSync } = await import('node:child_process');
  const rel = FILES[0]!; // 24/96
  // 헤드룸 없이 +9dB를 올리면 반드시 넘친다
  const dsp = encodeRequest({
    v: 1,
    ci: 1,
    src: { r: 48000, f: 'precise', p: 28 },
    out: { b: 24, d: 'tpdf' },
    f: [{ t: 'peq', pre: 9, b: [{ type: 'peak', freq: 1000, gain: 3, q: 1, on: true }] }],
  });
  const sid = 'test-session-1';
  const res = await call(ids.get(rel)!, {}, `?dsp=${dsp}&sid=${sid}&t=10`);
  assert.equal(res.status, 200);
  assert.equal(res.headers.get('x-sori-rate'), '48000');
  const flac = Buffer.from(await res.arrayBuffer());
  const probe = spawnSync(
    'ffprobe',
    ['-v', 'error', '-show_entries', 'stream=sample_rate,bits_per_raw_sample:format=duration', '-of', 'json', '-i', 'pipe:0'],
    { input: flac },
  );
  const info = JSON.parse(probe.stdout.toString());
  assert.equal(info.streams[0].sample_rate, '48000');
  assert.equal(info.streams[0].bits_per_raw_sample, '24');
  const clips = getClips(sid);
  assert.ok(clips.times.length > 0, '클리핑 시각이 기록된다');
  assert.ok(
    clips.times.every((t) => t >= 10),
    '시각은 곡 기준(시작 위치 10초 이후)',
  );
  assert.ok(clips.peak! > 0);
});

test('DSP: 임펄스 응답을 올려 컨볼루션을 건다', { skip }, async () => {
  const { spawnSync } = await import('node:child_process');
  const { saveIr, listIrs, deleteIr } = await import('../lib/dsp/ir');
  const { encodeRequest } = await import('../lib/dsp/request');
  // 단순한 임펄스 응답: 첫 샘플 1, 잠깐 뒤 작은 메아리
  const wav = spawnSync(
    'ffmpeg',
    [
      '-v',
      'error',
      '-f',
      'lavfi',
      '-i',
      'aevalsrc=if(eq(n\\,0)\\,1\\,if(eq(n\\,2400)\\,0.3\\,0)):s=48000:d=0.1',
      '-c:a',
      'pcm_f32le',
      '-f',
      'wav',
      '-',
    ],
    { maxBuffer: 1 << 24 },
  ).stdout;
  const ir = await saveIr('room.wav', new Uint8Array(wav));
  assert.equal(ir.channels, 1);
  assert.equal((await listIrs())[0]!.id, ir.id);
  const dsp = encodeRequest({
    v: 1,
    hr: -3,
    out: { b: 24, d: 'tpdf' },
    f: [{ t: 'convolution', ir: ir.id, g: 0, n: true, name: 'room.wav' }],
  });
  const res = await call(ids.get(FILES[3]!)!, {}, `?dsp=${dsp}`);
  assert.equal(res.status, 200);
  const flac = Buffer.from(await res.arrayBuffer());
  const decoded = spawnSync('ffmpeg', ['-v', 'error', '-i', 'pipe:0', '-f', 'null', '-'], { input: flac });
  assert.equal(decoded.status, 0);
  assert.ok(flac.length > 100_000);
  assert.equal(await deleteIr(ir.id), true);
});

test('DSP: 망가진 요청은 무시하고 원본을 보낸다', { skip }, async () => {
  const res = await call(ids.get(FILES[0]!)!, {}, '?dsp=garbage');
  assert.equal(res.headers.get('content-type'), 'audio/flac');
  assert.equal(res.headers.get('x-sori-dsp'), null);
  const body = Buffer.from(await res.arrayBuffer());
  assert.ok(body.equals(readFileSync(path.join(music, FILES[0]!))));
});
