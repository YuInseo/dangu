import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { test } from 'node:test';
import { parseAutoEq } from '../src/lib/dsp/autoeq';
import { coeffs, magnitudeDb, maxBoost } from '../src/lib/dsp/biquad';
import { defaultConfig } from '../src/lib/dsp/defaults';
import { levelingGain } from '../src/lib/dsp/leveling';
import { planDsp } from '../src/lib/dsp/plan';
import { buildRequest, encodeRequest, parseRequest } from '../src/lib/dsp/request';
import { srcTarget } from '../src/lib/dsp/src';
import type { Band, DspConfig, DspRequest } from '../src/lib/dsp/types';

const hasFfmpeg = spawnSync('ffmpeg', ['-version']).status === 0;
const ctx = { sourceRate: 96000, deviceRate: 48000, sameAlbumNeighbors: false };

test('아무것도 켜지 않으면 원본 그대로', () => {
  assert.equal(buildRequest(defaultConfig(), ctx), null);
  const c = defaultConfig();
  c.enabled = true; // 헤드룸 -3dB가 기본으로 켜져 있다
  assert.equal(buildRequest(c, ctx)?.hr, -3);
  c.headroom.gain = 0;
  assert.equal(buildRequest(c, ctx), null, '헤드룸 0dB만으로는 아무것도 바꾸지 않는다');
});

test('레벨링 자동: 같은 앨범이 이어지면 앨범 기준', () => {
  const c = defaultConfig();
  c.leveling.mode = 'auto';
  assert.equal(buildRequest(c, ctx)?.lv?.m, 'track');
  assert.equal(buildRequest(c, { ...ctx, sameAlbumNeighbors: true })?.lv?.m, 'album');
});

test('요청은 주소로 갔다 와도 같고, 이상한 값은 걸러진다', () => {
  const c: DspConfig = defaultConfig();
  c.enabled = true;
  c.src.enabled = true;
  const peq = c.filters.find((f) => f.type === 'peq')!;
  peq.enabled = true;
  if (peq.type === 'peq') peq.bands = [{ type: 'peak', freq: 1000, gain: 3, q: 1, on: true }];
  const req = buildRequest(c, ctx)!;
  assert.deepEqual(parseRequest(encodeRequest(req)), req);
  assert.equal(req.src?.r, 48000);

  const evil = {
    v: 1,
    hr: '-3,volume=99',
    out: { b: 24, d: 'tpdf' },
    src: { r: 12345, f: 'precise', p: 28 },
    f: [
      { t: 'peq', pre: 0, b: [{ type: 'peak;rm -rf', freq: '1000:g=99', gain: 1e9, q: -5, on: true }] },
      { t: 'convolution', ir: '../../etc/passwd', g: 0, n: true },
    ],
  };
  const parsed = parseRequest(btoa(JSON.stringify(evil)))!;
  assert.equal(parsed.hr, 0, '숫자가 아니면 안전한 기본값');
  assert.equal(parsed.src, undefined, '표준이 아닌 샘플레이트는 버린다');
  assert.equal(parsed.f.length, 1, '경로 같은 IR 이름은 버린다');
  const b = (parsed.f[0] as Extract<DspRequest['f'][number], { t: 'peq' }>).b[0]!;
  assert.deepEqual(b, { type: 'peak', freq: 1000, gain: 30, q: 0.05, on: true });
  assert.equal(parseRequest('not base64 json'), null);
});

test('볼륨 레벨링: 목표까지, 올릴 때는 트루 피크에서 멈춘다', () => {
  const loud = { track: { lufs: -9.9, peak: -1 }, source: 'analysis' as const };
  assert.deepEqual(levelingGain(loud, 'track', -14, -5), { gain: -4.1, basis: 'track', limited: false, lufs: -9.9 });
  const quiet = { track: { lufs: -20, peak: -1 }, source: 'analysis' as const };
  assert.equal(levelingGain(quiet, 'track', -14, -5).gain, 1);
  assert.equal(levelingGain(quiet, 'track', -14, -5).limited, true);
  assert.equal(levelingGain(undefined, 'track', -14, -5).gain, -5);
  assert.equal(levelingGain(quiet, 'album', -14, -5).basis, 'track', '앨범 값이 없으면 트랙 값');
});

test('샘플레이트 변환의 목표', () => {
  const src = { enabled: true, filter: 'precise' as const, precision: 28 as const, maxRate: 192000, custom: { '44100': 88200 } };
  assert.equal(srcTarget({ ...src, mode: 'match' }, 96000, 48000), 48000);
  assert.equal(srcTarget({ ...src, mode: 'match' }, 48000, 48000), null);
  assert.equal(srcTarget({ ...src, mode: 'max2' }, 44100, null), 176400);
  assert.equal(srcTarget({ ...src, mode: 'max2' }, 48000, null), 192000);
  assert.equal(srcTarget({ ...src, mode: 'compat', maxRate: 96000 }, 192000, null), 96000);
  assert.equal(srcTarget({ ...src, mode: 'compat', maxRate: 96000 }, 44100, null), null);
  assert.equal(srcTarget({ ...src, mode: 'custom' }, 44100, null), 88200);
});

test('AutoEQ 붙여넣기', () => {
  const r = parseAutoEq(`Preamp: -6.2 dB
Filter 1: ON LSC Fc 105 Hz Gain 5.5 dB Q 0.70
Filter 2: ON PK Fc 2300 Hz Gain -3.1 dB Q 1.41
Filter 3: OFF HSC Fc 10000 Hz Gain -2.0 dB Q 0.70
Filter 4: ON XX Fc 10 Hz Gain 0 dB Q 1`);
  assert.equal(r.preamp, -6.2);
  assert.deepEqual(r.bands[0], { type: 'lowshelf', freq: 105, gain: 5.5, q: 0.7, on: true });
  assert.equal(r.bands[1]!.type, 'peak');
  assert.equal(r.bands[2]!.on, false);
  assert.equal(r.skipped, 1);
});

test('최대 부스트: 헤드룸을 얼마나 잡아야 하는지', () => {
  assert.ok(Math.abs(maxBoost([{ type: 'peak', freq: 1000, gain: 6, q: 1, on: true }]) - 6) < 0.05);
  assert.ok(Math.abs(maxBoost([{ type: 'peak', freq: 1000, gain: 6, q: 1, on: true }], -6)) < 0.05);
});

/** ffmpeg에 사인파를 통과시켜 실제 이득을 잰다. */
function measuredGain(filter: string, freq: number, fs = 48000): number {
  const run = (af: string) => {
    const out = spawnSync(
      'ffmpeg',
      [
        '-v',
        'error',
        '-f',
        'lavfi',
        '-i',
        `aevalsrc=0.25*sin(2*PI*${freq}*t):s=${fs}:d=1`,
        '-af',
        `aformat=sample_fmts=dblp,${af}`,
        '-f',
        'f64le',
        '-',
      ],
      {
        maxBuffer: 1 << 26,
      },
    ).stdout;
    const x = new Float64Array(out.buffer, out.byteOffset, out.length / 8).subarray(fs / 2); // 앞의 과도 응답은 버린다
    return Math.sqrt(x.reduce((s, v) => s + v * v, 0) / x.length);
  };
  return 20 * Math.log10(run(filter) / run('anull'));
}

test('화면의 EQ 곡선 = ffmpeg가 실제로 거는 필터', { skip: !hasFfmpeg && 'ffmpeg 없음' }, () => {
  const fs = 48000;
  const cases: Band[] = [
    { type: 'peak', freq: 1000, gain: 6, q: 1.4, on: true },
    { type: 'lowshelf', freq: 120, gain: 5, q: 0.7, on: true },
    { type: 'highshelf', freq: 8000, gain: -4, q: 0.7, on: true },
    { type: 'lowpass', freq: 5000, gain: 0, q: 0.707, on: true },
  ];
  const req: DspRequest = { v: 1, out: { b: 24, d: 'none' }, f: [] };
  for (const b of cases) {
    const graph = planDsp(
      { ...req, f: [{ t: 'peq', pre: 0, b: [b] }] },
      { codec: 'FLAC', lossless: true, sampleRate: fs },
      undefined,
    ).graph;
    const filter = /(equalizer|lowshelf|highshelf|lowpass)=[^,\[]+/.exec(graph)![0];
    for (const f of [100, 1000, 3000, 10000]) {
      const expected = magnitudeDb(coeffs(b, fs), f, fs);
      const got = measuredGain(filter, f, fs);
      assert.ok(Math.abs(expected - got) < 0.05, `${b.type} @${f}Hz: 곡선 ${expected.toFixed(2)}dB, ffmpeg ${got.toFixed(2)}dB`);
    }
  }
});

test('모든 필터를 켠 그래프가 ffmpeg에서 실제로 돈다', { skip: !hasFfmpeg && 'ffmpeg 없음' }, () => {
  const req: DspRequest = {
    v: 1,
    hr: -3,
    ci: 1,
    lv: { m: 'track', t: -14, u: -5 },
    src: { r: 48000, f: 'smooth', p: 28 },
    out: { b: 24, d: 'tpdf' },
    f: [
      { t: 'crossfeed', p: 'jmeier' },
      { t: 'crossfeed', p: 'custom', fc: 800, fd: 6 },
      {
        t: 'peq',
        pre: -2,
        b: [
          { type: 'peak', freq: 1000, gain: 3, q: 1, on: true },
          { type: 'highpass', freq: 25, gain: 0, q: 0.7, on: true },
        ],
      },
      {
        t: 'procedural',
        s: [
          { op: 'volume', ch: 'L', gain: -1.5 },
          { op: 'volume', ch: 'LR', gain: -0.5 },
          { op: 'delay', ch: 'R', ms: 0.75 },
          { op: 'invert', ch: 'R' },
          { op: 'mute', ch: 'L' },
          { op: 'mix', ll: 0.5, lr: 0.5, rl: -0.25, rr: 1 },
          { op: 'eq', ch: 'L', band: { type: 'notch', freq: 60, gain: 0, q: 10, on: true } },
        ],
      },
      { t: 'headphone', pre: -6, b: [{ type: 'lowshelf', freq: 105, gain: 5.5, q: 0.7, on: true }], m: 'Test HP' },
    ],
  };
  const plan = planDsp(
    req,
    { codec: 'FLAC', lossless: true, sampleRate: 96000, bitDepth: 24, channels: 2 },
    { track: { lufs: -10, peak: -0.5 }, source: 'analysis' },
  );
  const r = spawnSync(
    'ffmpeg',
    [
      '-v',
      'error',
      '-f',
      'lavfi',
      '-i',
      'aevalsrc=0.5*sin(2*PI*440*t)|0.5*sin(2*PI*550*t):s=96000:d=0.5',
      '-filter_complex',
      plan.graph,
      '-map',
      '[out]',
      '-c:a',
      'flac',
      '-sample_fmt',
      's32',
      '-bits_per_raw_sample',
      '24',
      '-f',
      'flac',
      '-',
    ],
    {
      maxBuffer: 1 << 26,
    },
  );
  assert.equal(r.status, 0, r.stderr.toString());
  assert.ok(r.stdout.length > 1000);
  assert.equal(plan.outRate, 48000);
  assert.deepEqual(
    plan.stages.map((s) => [s.title, s.quality]),
    [
      ['비트 깊이 변환', 'lossless'],
      ['볼륨 레벨링', 'enhanced'],
      ['헤드룸 조정', 'enhanced'],
      ['크로스피드', 'enhanced'],
      ['크로스피드', 'enhanced'],
      ['파라메트릭 EQ', 'enhanced'],
      ['프로시저럴 EQ', 'enhanced'],
      ['헤드폰 EQ', 'enhanced'],
      ['샘플레이트 변환', 'high'],
      ['비트 깊이 변환', 'high'],
    ],
  );
  assert.match(plan.stages[1]!.detail, /−4\.0dB/);
});
