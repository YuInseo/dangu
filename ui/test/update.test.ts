import assert from 'node:assert/strict';
import { test } from 'node:test';
import { buildName, upToDateText } from '../src/lib/update';

test('빌드는 번호로 부른다', () => {
  assert.equal(buildName(19, '268e2253869a1134f6008e76a0bb0471bab90e52'), '빌드 19');
  assert.equal(buildName(null, '268e2253869a1134f6008e76a0bb0471bab90e52'), '빌드 268e225', '번호가 없는 옛 빌드');
});

test('새 버전이 없을 때: 이 기기의 것과 올라온 것을 나란히', () => {
  const latest = { build: 'aaa', number: 19, time: 200, notes: '', size: 1 };
  assert.match(upToDateText({ build: 'aaa', number: 19, buildTime: 200, latest }), /지금 쓰는 것이 올라온 가장 새 빌드\(빌드 19\)/);
  const older = { ...latest, build: 'old', number: 17, time: 100 };
  const t = upToDateText({ build: 'aaa', number: 19, buildTime: 200, latest: older });
  assert.match(t, /올라온 것은 빌드 17/);
  assert.match(t, /이 기기는 빌드 19/);
  assert.equal(upToDateText({ build: 'aaa', number: null, buildTime: 0, latest: null }), '최신 버전입니다(빌드 aaa)');
});
