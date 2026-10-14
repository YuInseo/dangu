import assert from 'node:assert/strict';
import { existsSync, mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { after, before, test } from 'node:test';

const music = path.resolve('music/Sori Samples');
const skip = !existsSync(music) && '시험용 라이브러리가 없습니다 (npm run sample)';
const data = mkdtempSync(path.join(tmpdir(), 'sori-test-'));
process.env.MUSIC_DIR = music;
process.env.DATA_DIR = data;

type Lib = import('../lib/library/build').Library;
let lib: Lib;
let index: import('../lib/library/scanner').IndexFile;

before(async () => {
  if (skip) return;
  const { scanLibrary } = await import('../lib/library/scanner');
  const { buildLibrary } = await import('../lib/library/build');
  const r = await scanLibrary(null, () => {});
  index = r.index;
  lib = buildLibrary(index);
});

after(() => rmSync(data, { recursive: true, force: true }));

const album = (title: string) => lib.albumList.find((a) => a.title === title)!;

test('앨범 일곱 장, 곡 스물두 개', { skip }, () => {
  assert.equal(lib.summary.tracks, 22);
  assert.deepEqual(
    lib.albumList.map((a) => a.title).sort(),
    ['Field Notes - Rain Studies', 'Glass Garden', 'Lo-Fi Sketches', 'Night Drive', 'Paper Moon', 'Sori Sampler', '아침의 차'].sort(),
  );
});

test('디스크 두 장짜리 컴필레이션은 한 앨범', { skip }, () => {
  const a = album('Sori Sampler');
  assert.equal(a.artist, 'Various Artists');
  assert.equal(a.discCount, 2);
  assert.equal(a.trackIds.length, 4);
  const discs = a.trackIds.map((id) => lib.views.get(id)!.discNo);
  assert.deepEqual(discs, [1, 1, 2, 2]);
  assert.ok(a.cover, '부모 폴더의 cover.jpg');
});

test('태그 없는 WAV는 폴더와 파일 이름에서', { skip }, () => {
  const a = album('Field Notes - Rain Studies');
  const ts = a.trackIds.map((id) => lib.views.get(id)!);
  assert.deepEqual(
    ts.map((t) => [t.trackNo, t.title]),
    [
      [1, 'Rain on Tin'],
      [2, 'Gutter Song'],
    ],
  );
  assert.equal(a.cover, undefined);
});

test('포맷과 등급', { skip }, () => {
  assert.equal(album('Night Drive').format.sampleRate, 96000);
  assert.equal(album('Night Drive').format.bitDepth, 24);
  assert.equal(album('Glass Garden').format.codec, 'ALAC');
  assert.equal(album('Paper Moon').format.codec, 'AIFF');
  assert.equal(album('Lo-Fi Sketches').format.codec, 'MP3');
  assert.equal(album('Lo-Fi Sketches').format.lossless, false);
  assert.equal(album('아침의 차').format.bitDepth, 16);
});

test('표지: 박힌 것과 폴더의 것', { skip }, () => {
  for (const t of ['Night Drive', 'Glass Garden', 'Lo-Fi Sketches', '아침의 차', 'Paper Moon']) assert.ok(album(t).cover, t);
  assert.match(album('Night Drive').cover!.color, /^#[0-9a-f]{6}$/);
});

test('아티스트: 앨범 아티스트와 참여곡', { skip }, () => {
  const neon = lib.artistList.find((a) => a.name === 'Neon Harbor')!;
  assert.equal(neon.albumIds.length, 2);
  assert.equal(neon.appearsOn.length, 1, '컴필레이션의 한 곡');
  assert.ok(!lib.artistList.some((a) => a.name === 'Various Artists'));
});

test('검색', { skip }, async () => {
  const { searchLibrary } = await import('../lib/library/build');
  assert.equal(searchLibrary(lib, '아침').albums[0]?.title, '아침의 차');
  assert.equal(searchLibrary(lib, 'neon night').albums[0]?.title, 'Night Drive');
  assert.ok(searchLibrary(lib, 'chrome rain').tracks.some((t) => t.title === 'Chrome Rain'));
});

test('다시 스캔하면 바뀐 게 없으니 그대로 쓴다', { skip }, async () => {
  const { scanLibrary } = await import('../lib/library/scanner');
  const r = await scanLibrary(index, () => {});
  assert.equal(r.added, 0);
  assert.equal(r.removed, 0);
  assert.deepEqual(r.index.covers, index.covers);
});
