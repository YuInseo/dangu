import assert from 'node:assert/strict';
import { test } from 'node:test';
import { agoIso, compact, EMBED_OFF, internalHref, length, linkify, playerFailure, subsText, viewsText } from '../src/lib/youtube';

test('수는 한국식으로 줄인다', () => {
  assert.equal(compact(999), '999');
  assert.equal(compact(1234), '1.2천');
  assert.equal(compact(98_123_456), '9812만');
  assert.equal(compact(152_004_311), '1.5억');
  assert.equal(viewsText(290_123_456), '조회수 2.9억회');
  assert.equal(subsText(76_400_000), '구독자 7640만명');
  assert.equal(viewsText(undefined), null);
});

test('올린 때와 길이', () => {
  const now = Date.parse('2026-10-18T09:00:00Z');
  assert.equal(agoIso('2026-10-18T08:59:30Z', now), '방금');
  assert.equal(agoIso('2026-10-18T06:00:00Z', now), '3시간 전');
  assert.equal(agoIso('2026-10-15T09:00:00Z', now), '3일 전');
  assert.equal(agoIso('2026-09-20T09:00:00Z', now), '4주 전');
  assert.equal(agoIso('2025-10-01T09:00:00Z', now), '1년 전');
  assert.equal(agoIso('nope', now), null);
  assert.equal(length(201), '3:21');
  assert.equal(length(3723), '1:02:03');
  assert.equal(length(0), null);
});

test('설명의 주소와 시각', () => {
  const p = linkify('00:00 인트로\n1:32 후렴 https://example.com/listen. 1:02:03까지, 10:5 아님');
  assert.deepEqual(
    p.filter((x) => x.t !== 'text'),
    [
      { t: 'time', v: '00:00', sec: 0 },
      { t: 'time', v: '1:32', sec: 92 },
      { t: 'url', v: 'https://example.com/listen' },
      { t: 'time', v: '1:02:03', sec: 3723 },
    ],
  );
  assert.equal(p.map((x) => x.v).join(''), '00:00 인트로\n1:32 후렴 https://example.com/listen. 1:02:03까지, 10:5 아님');
  assert.equal(internalHref('https://www.youtube.com/watch?v=ArmDp-zijuc&t=1'), '/youtube/watch/ArmDp-zijuc');
  assert.equal(internalHref('https://youtu.be/ArmDp-zijuc'), '/youtube/watch/ArmDp-zijuc');
  assert.equal(internalHref('https://www.youtube.com/channel/UCMki_UkHb4qSc0qyEcOHHJw'), '/youtube/channel/UCMki_UkHb4qSc0qyEcOHHJw');
  assert.equal(internalHref('https://example.com'), null);
});

test('틀지 못한 까닭', () => {
  assert.equal(playerFailure(150).title, EMBED_OFF.title);
  assert.match(playerFailure(150).detail ?? '', /음원 권리자/);
  assert.equal(playerFailure(101).code, 101);
  assert.equal(playerFailure(100).title, '영상을 찾을 수 없습니다');
  assert.deepEqual(playerFailure(5), { title: '이 기기에서 재생할 수 없는 영상입니다', code: 5 });
  assert.equal(EMBED_OFF.code, undefined, '영상 정보로 미리 안 것에는 번호가 없다');
});
