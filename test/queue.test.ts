import assert from 'node:assert/strict';
import { test } from 'node:test';
import { initialQueue, nextIndex, queueReducer, wrap, type QueueState } from '../lib/player/queue';
import type { PlayableTrack } from '../lib/player/types';

const track = (n: number): PlayableTrack => ({
  key: `lib:${n}`,
  source: 'library',
  title: `곡 ${n}`,
  artist: 'A',
  album: 'B',
  duration: 100,
  format: { codec: 'FLAC', container: 'flac', lossless: true, mime: 'audio/flac' },
  src: `/api/stream/${n}`,
  art: { seed: 'x', title: 'B' },
});

const tracks = (n: number) => Array.from({ length: n }, (_, i) => track(i));
const keys = (s: QueueState) => s.items.map((i) => i.track.key);

test('고른 곡부터 재생하고, 끝나면 멈춘다', () => {
  const s = queueReducer(initialQueue, { type: 'replace', items: wrap(tracks(4)), index: 2 });
  assert.equal(s.index, 2);
  assert.equal(nextIndex(s), 3);
  assert.equal(nextIndex({ ...s, index: 3 }), null);
  assert.equal(nextIndex({ ...s, index: 3, repeat: 'all' }), 0);
});

test('셔플 버튼은 모든 곡을 한 번씩, 끄면 원래 순서와 지금 곡으로', () => {
  let s = queueReducer(initialQueue, { type: 'replace', items: wrap(tracks(20)), index: 0, shuffle: true });
  assert.equal(s.shuffle, true);
  assert.deepEqual(
    [...keys(s)].sort(),
    tracks(20)
      .map((t) => t.key)
      .sort(),
  );
  s = queueReducer(s, { type: 'jump', index: 5 });
  const cur = s.items[s.index]!.uid;
  s = queueReducer(s, { type: 'toggleShuffle' });
  assert.equal(s.shuffle, false);
  assert.deepEqual(
    keys(s),
    tracks(20).map((t) => t.key),
  );
  assert.equal(s.items[s.index]!.uid, cur);
});

test('셔플을 켜도 지금 곡과 지난 곡은 제자리', () => {
  let s = queueReducer(initialQueue, { type: 'replace', items: wrap(tracks(10)), index: 3 });
  const before = keys(s).slice(0, 4);
  s = queueReducer(s, { type: 'toggleShuffle' });
  assert.deepEqual(keys(s).slice(0, 4), before);
  assert.equal(s.index, 3);
});

test('다음에 재생은 지금 곡 바로 뒤에, 셔플을 꺼도 그 자리를 지킨다', () => {
  let s = queueReducer(initialQueue, { type: 'replace', items: wrap(tracks(5)), index: 1 });
  s = queueReducer(s, { type: 'toggleShuffle' });
  s = queueReducer(s, { type: 'insertNext', items: wrap([track(99)]) });
  assert.equal(s.items[s.index + 1]!.track.key, 'lib:99');
  s = queueReducer(s, { type: 'toggleShuffle' });
  assert.deepEqual(keys(s), ['lib:0', 'lib:1', 'lib:99', 'lib:2', 'lib:3', 'lib:4']);
});

test('지금 곡을 빼면 다음 곡이 지금 곡이 되고 다시 건다', () => {
  let s = queueReducer(initialQueue, { type: 'replace', items: wrap(tracks(3)), index: 1 });
  const token = s.token;
  s = queueReducer(s, { type: 'remove', index: 1 });
  assert.deepEqual(keys(s), ['lib:0', 'lib:2']);
  assert.equal(s.index, 1);
  assert.equal(s.token, token + 1);
});

test('앞의 곡을 빼면 지금 곡은 그대로', () => {
  let s = queueReducer(initialQueue, { type: 'replace', items: wrap(tracks(4)), index: 2 });
  const token = s.token;
  s = queueReducer(s, { type: 'remove', index: 0 });
  assert.equal(s.items[s.index]!.track.key, 'lib:2');
  assert.equal(s.token, token);
});

test('순서를 바꿔도 지금 곡을 따라간다', () => {
  let s = queueReducer(initialQueue, { type: 'replace', items: wrap(tracks(5)), index: 1 });
  s = queueReducer(s, { type: 'move', from: 4, to: 0 });
  assert.deepEqual(keys(s), ['lib:4', 'lib:0', 'lib:1', 'lib:2', 'lib:3']);
  assert.equal(s.items[s.index]!.track.key, 'lib:1');
});

test('빈 대기열에 추가하면 그 곡이 지금 곡이 된다', () => {
  const s = queueReducer(initialQueue, { type: 'append', items: wrap([track(7)]) });
  assert.equal(s.items[s.index]!.track.key, 'lib:7');
  assert.equal(s.token, 1);
});
