import assert from 'node:assert/strict';
import { beforeEach, test } from 'node:test';
import { isLiked, libraryId, libraryIds, moveItem, noteCollections, refreshFavorites, resetCollectionsForTest, setLiked } from '../src/lib/collections';
import type { PlayableTrack } from '../src/lib/player/types';
import { ago } from '../src/lib/time';

const lib = (id: string): PlayableTrack => ({
  key: `lib:${id}`,
  source: 'library',
  title: id,
  artist: 'A',
  album: 'B',
  duration: 100,
  format: { codec: 'FLAC', container: 'flac', lossless: true, mime: 'audio/flac' },
  src: `/api/stream/${id}`,
  art: { seed: 'x', title: 'B' },
});

const live: PlayableTrack = { ...lib('x'), key: 'live:gd1977/d1t01.flac', source: 'live', live: { id: 'gd1977', file: 'd1t01.flac' } };

/** 서버 흉내: 좋아요 목록과, 곡마다 성공·실패·늦게 답하기 */
function fakeServer(server: { tracks: string[]; albums: string[] }, opts: { fail?: Set<string>; hold?: Map<string, Promise<void>> } = {}) {
  const calls: string[] = [];
  globalThis.fetch = (async (url: string, init?: RequestInit) => {
    const method = init?.method ?? 'GET';
    calls.push(`${method} ${url}`);
    if (url === '/api/favorites/ids') return Response.json(server);
    const m = /^\/api\/favorites\/(tracks|albums)\/(.+)$/.exec(url);
    if (m) {
      const [, kind, id] = m as unknown as [string, 'tracks' | 'albums', string];
      await opts.hold?.get(id);
      if (opts.fail?.has(id)) return Response.json({ error: '안 됨' }, { status: 500 });
      const list = server[kind].filter((x) => x !== id);
      server[kind] = method === 'PUT' ? [id, ...list] : list;
      return Response.json({ liked: method === 'PUT' });
    }
    return Response.json({ error: 'no' }, { status: 404 });
  }) as typeof fetch;
  return calls;
}

beforeEach(() => resetCollectionsForTest());

test('라이브러리 곡만 좋아요·플레이리스트에 넣는다', () => {
  assert.equal(libraryId(lib('abc')), 'abc');
  assert.equal(libraryId(live), undefined);
  assert.deepEqual(libraryIds([lib('a'), live, lib('b')]), ['a', 'b']);
});

test('한 칸 옮기기는 새 배열을 돌려주고, 범위 밖이면 그대로', () => {
  const xs = ['a', 'b', 'c', 'd'];
  assert.deepEqual(moveItem(xs, 0, 2), ['b', 'c', 'a', 'd']);
  assert.deepEqual(moveItem(xs, 3, 0), ['d', 'a', 'b', 'c']);
  assert.deepEqual(moveItem(xs, 1, 9), xs);
  assert.deepEqual(xs, ['a', 'b', 'c', 'd'], '원래 배열은 그대로');
});

test('좋아요는 먼저 바뀌고, 서버가 못 받으면 되돌린다', async () => {
  fakeServer({ tracks: ['a'], albums: [] }, { fail: new Set(['bad']) });
  await refreshFavorites();
  assert.ok(isLiked('tracks', 'a'));
  const p = setLiked('tracks', 'b', true);
  assert.ok(isLiked('tracks', 'b'), '기다리지 않고 바로');
  await p;
  await assert.rejects(setLiked('tracks', 'bad', true));
  assert.ok(!isLiked('tracks', 'bad'), '실패하면 되돌린다');
  await setLiked('tracks', 'a', false);
  assert.ok(!isLiked('tracks', 'a'));
});

test('답을 기다리는 동안 받은 목록이 누른 것을 덮지 않는다', async () => {
  let release!: () => void;
  const hold = new Map([['c', new Promise<void>((r) => (release = r))]]);
  fakeServer({ tracks: [], albums: [] }, { hold });
  const p = setLiked('tracks', 'c', true);
  await refreshFavorites();
  assert.ok(isLiked('tracks', 'c'), '서버 목록에는 아직 없지만 누른 대로');
  release();
  await p;
  await refreshFavorites();
  assert.ok(isLiked('tracks', 'c'));
});

test('다른 화면에서 바꾸면(세대가 오르면) 다시 받는다', async () => {
  const server = { tracks: [] as string[], albums: [] as string[] };
  const calls = fakeServer(server);
  noteCollections(5);
  assert.equal(calls.length, 0, '처음 본 세대로는 받지 않는다');
  server.albums = ['al'];
  noteCollections(5);
  assert.equal(calls.length, 0);
  noteCollections(6);
  await refreshFavorites();
  assert.ok(isLiked('albums', 'al'));
});

test('지난 때를 사람 말로', () => {
  const now = Date.UTC(2026, 9, 15, 12);
  assert.equal(ago(now - 20_000, now), '방금');
  assert.equal(ago(now - 5 * 60_000, now), '5분 전');
  assert.equal(ago(now - 3 * 3600_000, now), '3시간 전');
  assert.equal(ago(now - 30 * 3600_000, now), '어제');
  assert.equal(ago(now - 4 * 86400_000, now), '4일 전');
});
