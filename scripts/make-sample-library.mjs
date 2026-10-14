#!/usr/bin/env node
/**
 * 시험용 라이브러리를 만든다.
 *
 * 곡은 전부 여기서 합성한다 — 저작권 걱정 없이 저장소 밖에서 마음대로 틀어 볼 수 있다.
 * 포맷을 일부러 골고루 섞었다. FLAC 24/96(Hi-Res), FLAC 16/44.1, ALAC, AIFF, WAV, MP3,
 * 그리고 디스크 두 장짜리 컴필레이션까지. 표지는 곡에 박힌 것, 폴더의 cover.jpg,
 * 아예 없는 것이 섞여 있다.
 *
 *   node scripts/make-sample-library.mjs [출력 폴더=./music/Sori Samples] [--short]
 *
 * ffmpeg가 필요하다.
 */
import { spawn, spawnSync } from 'node:child_process';
import fs from 'node:fs/promises';
import path from 'node:path';
import sharp from 'sharp';

const args = process.argv.slice(2);
const short = args.includes('--short');
const out = path.resolve(args.find((a) => !a.startsWith('--')) ?? 'music/Sori Samples');
const SECONDS = short ? 12 : 42;

if (spawnSync('ffmpeg', ['-version']).status !== 0) {
  console.error('ffmpeg가 없습니다. 먼저 설치하세요.');
  process.exit(1);
}

// ---------------------------------------------------------------- 무작위(고정 씨앗)

function rng(seed) {
  let s = seed >>> 0;
  return () => {
    s = (s + 0x6d2b79f5) >>> 0;
    let t = s;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

function seedOf(s) {
  let h = 2166136261;
  for (const c of s) h = Math.imul(h ^ c.charCodeAt(0), 16777619);
  return h >>> 0;
}

// ---------------------------------------------------------------- 합성

const TABLE = 4096;

/** 배음 진폭으로 한 주기짜리 파형을 미리 만든다. 샘플마다 sin을 수십 번 부르지 않기 위해서다. */
function wavetable(harmonics) {
  const t = new Float32Array(TABLE + 1);
  for (let i = 0; i <= TABLE; i++) {
    let v = 0;
    harmonics.forEach((a, k) => (v += a * Math.sin((2 * Math.PI * (k + 1) * i) / TABLE)));
    t[i] = v;
  }
  let peak = 0;
  for (const v of t) peak = Math.max(peak, Math.abs(v));
  for (let i = 0; i <= TABLE; i++) t[i] /= peak;
  return t;
}

const midiHz = (m) => 440 * 2 ** ((m - 69) / 12);

const SCALES = {
  minor: [0, 2, 3, 5, 7, 8, 10],
  major: [0, 2, 4, 5, 7, 9, 11],
  dorian: [0, 2, 3, 5, 7, 9, 10],
};

/**
 * 패드 + 아르페지오 + 베이스, 아르페지오에는 핑퐁 딜레이.
 * 스테레오 Float32 인터리브로 돌려준다.
 */
function synth({ seed, sampleRate, seconds, mood }) {
  const r = rng(seed);
  const scale = SCALES[mood.scale];
  const root = 45 + Math.floor(r() * 10);
  const bpm = mood.bpm[0] + Math.floor(r() * (mood.bpm[1] - mood.bpm[0]));
  const beat = 60 / bpm;
  const bar = beat * 4;
  const degreesPool = [
    [0, 5, 3, 4],
    [0, 3, 5, 4],
    [5, 3, 0, 4],
    [0, 4, 5, 3],
    [0, 2, 5, 4],
  ];
  const prog = degreesPool[Math.floor(r() * degreesPool.length)];
  const note = (deg, oct = 0) => root + 12 * (oct + Math.floor(deg / 7)) + scale[((deg % 7) + 7) % 7];

  const padTable = wavetable(mood.pad);
  const pluckTable = wavetable([1, 0.35, 0.12, 0.05]);
  const bassTable = wavetable([1, 0.25, 0.08]);

  const n = Math.floor(seconds * sampleRate);
  const L = new Float32Array(n);
  const R = new Float32Array(n);

  const voice = (table, hz, start, dur, gain, attack, release, panL, panR, decay = 0) => {
    const s0 = Math.floor(start * sampleRate);
    const len = Math.floor((dur + release) * sampleRate);
    const inc = (hz * TABLE) / sampleRate;
    let ph = r() * TABLE;
    for (let i = 0; i < len && s0 + i < n; i++) {
      const t = i / sampleRate;
      let env = t < attack ? t / attack : t < dur ? 1 : Math.max(0, 1 - (t - dur) / release);
      if (decay) env *= Math.exp(-t / decay);
      const idx = ph | 0;
      const v = (table[idx] + (table[idx + 1] - table[idx]) * (ph - idx)) * env * gain;
      L[s0 + i] += v * panL;
      R[s0 + i] += v * panR;
      ph += inc;
      if (ph >= TABLE) ph -= TABLE;
    }
  };

  const bars = Math.ceil(seconds / bar);
  for (let b = 0; b < bars; b++) {
    const deg = prog[b % prog.length];
    const t0 = b * bar;
    // 패드: 세 음, 살짝 어긋난 두 겹
    for (const [i, d] of [deg, deg + 2, deg + 4].entries()) {
      const hz = midiHz(note(d, 1));
      voice(padTable, hz * 0.998, t0, bar, 0.07, 0.9, 1.2, 0.8 - i * 0.15, 0.5 + i * 0.15);
      voice(padTable, hz * 1.003, t0, bar, 0.07, 0.9, 1.2, 0.5 + i * 0.15, 0.8 - i * 0.15);
    }
    // 베이스
    voice(bassTable, midiHz(note(deg, -1)), t0, bar * 0.9, 0.22, 0.04, 0.3, 0.7, 0.7);
    // 아르페지오
    const steps = mood.arp;
    const pattern = [0, 2, 4, 7, 4, 2, 4, 6];
    for (let k = 0; k < steps; k++) {
      if (r() < mood.rest) continue;
      const d = deg + pattern[k % pattern.length];
      const t = t0 + (k * bar) / steps;
      const left = k % 2 === 0;
      voice(pluckTable, midiHz(note(d, 2)), t, 0.05, 0.16, 0.004, 0.6, left ? 0.9 : 0.4, left ? 0.4 : 0.9, mood.pluckDecay);
    }
  }

  // 핑퐁 딜레이
  const dly = Math.floor(beat * 0.75 * sampleRate);
  for (let i = dly; i < n; i++) {
    L[i] += R[i - dly] * 0.32;
    R[i] += L[i - dly] * 0.32;
  }

  // 시작과 끝을 부드럽게, 그리고 -1dBFS로 맞춘다
  const fade = Math.floor(1.5 * sampleRate);
  let peak = 0;
  for (let i = 0; i < n; i++) {
    const g = Math.min(1, i / fade, (n - 1 - i) / fade);
    L[i] = Math.tanh(L[i] * 1.2) * g;
    R[i] = Math.tanh(R[i] * 1.2) * g;
    peak = Math.max(peak, Math.abs(L[i]), Math.abs(R[i]));
  }
  const norm = 0.89 / (peak || 1);
  const out = new Float32Array(n * 2);
  for (let i = 0; i < n; i++) {
    out[2 * i] = L[i] * norm;
    out[2 * i + 1] = R[i] * norm;
  }
  return out;
}

// ---------------------------------------------------------------- 표지

async function makeCover(file, seed, style) {
  const r = rng(seed);
  const h1 = Math.floor(r() * 360);
  const h2 = (h1 + 40 + Math.floor(r() * 120)) % 360;
  const shapes = [];
  if (style === 'circles') {
    for (let i = 0; i < 7; i++) {
      shapes.push(
        `<circle cx="${200 + r() * 800}" cy="${200 + r() * 800}" r="${60 + r() * 260}" fill="hsl(${(h2 + i * 25) % 360} 70% ${45 + r() * 25}%)" opacity="${0.35 + r() * 0.4}"/>`,
      );
    }
  } else if (style === 'waves') {
    for (let i = 0; i < 14; i++) {
      const y = 140 + i * 62;
      const a = 20 + r() * 70;
      shapes.push(
        `<path d="M0 ${y} C 300 ${y - a}, 700 ${y + a}, 1200 ${y}" stroke="hsl(${(h2 + i * 8) % 360} 75% 70%)" stroke-width="${3 + r() * 6}" fill="none" opacity="${0.4 + r() * 0.5}"/>`,
      );
    }
  } else if (style === 'grid') {
    for (let x = 0; x < 6; x++)
      for (let y = 0; y < 6; y++)
        if (r() < 0.5)
          shapes.push(
            `<rect x="${x * 200 + 20}" y="${y * 200 + 20}" width="160" height="160" rx="${r() < 0.5 ? 80 : 12}" fill="hsl(${(h2 + x * 12 + y * 9) % 360} 65% ${40 + r() * 30}%)" opacity="0.85"/>`,
          );
  } else {
    shapes.push(`<circle cx="600" cy="600" r="380" fill="none" stroke="hsl(${h2} 80% 75%)" stroke-width="3" opacity=".6"/>`);
    for (let i = 0; i < 40; i++) {
      const a = (i / 40) * Math.PI * 2;
      const len = 60 + r() * 200;
      shapes.push(
        `<line x1="${600 + Math.cos(a) * 200}" y1="${600 + Math.sin(a) * 200}" x2="${600 + Math.cos(a) * (200 + len)}" y2="${600 + Math.sin(a) * (200 + len)}" stroke="hsl(${(h2 + i * 4) % 360} 80% 70%)" stroke-width="6" stroke-linecap="round" opacity=".8"/>`,
      );
    }
  }
  const svg = `<svg xmlns="http://www.w3.org/2000/svg" width="1200" height="1200">
    <defs><linearGradient id="g" x1="0" y1="0" x2="1" y2="1">
      <stop offset="0" stop-color="hsl(${h1} 55% 22%)"/><stop offset="1" stop-color="hsl(${h2} 50% 10%)"/>
    </linearGradient></defs>
    <rect width="1200" height="1200" fill="url(#g)"/>${shapes.join('')}</svg>`;
  await sharp(Buffer.from(svg)).jpeg({ quality: 90 }).toFile(file);
}

// ---------------------------------------------------------------- 인코딩

const FORMATS = {
  'flac-24-96': { ext: 'flac', sr: 96000, args: ['-c:a', 'flac', '-sample_fmt', 's32', '-bits_per_raw_sample', '24'] },
  'flac-24-48': { ext: 'flac', sr: 48000, args: ['-c:a', 'flac', '-sample_fmt', 's32', '-bits_per_raw_sample', '24'] },
  'flac-16-44': { ext: 'flac', sr: 44100, args: ['-c:a', 'flac', '-sample_fmt', 's16'] },
  'alac-24-48': { ext: 'm4a', sr: 48000, args: ['-c:a', 'alac', '-sample_fmt', 's32p'] },
  'aiff-16-44': { ext: 'aiff', sr: 44100, args: ['-c:a', 'pcm_s16be', '-write_id3v2', '1'] },
  'wav-16-44': { ext: 'wav', sr: 44100, args: ['-c:a', 'pcm_s16le'] },
  'mp3-320': { ext: 'mp3', sr: 44100, args: ['-c:a', 'libmp3lame', '-b:a', '320k', '-id3v2_version', '3'] },
};

function encode(file, pcm, fmt, tags, cover) {
  const f = FORMATS[fmt];
  const a = ['-hide_banner', '-loglevel', 'error', '-y', '-f', 'f32le', '-ar', String(f.sr), '-ac', '2', '-i', 'pipe:0'];
  const embed = cover && ['flac', 'mp3', 'm4a'].includes(f.ext);
  if (embed) a.push('-i', cover, '-map', '0:a', '-map', '1:v', '-c:v', 'copy', '-disposition:v', 'attached_pic');
  a.push(...f.args);
  for (const [k, v] of Object.entries(tags)) if (v != null) a.push('-metadata', `${k}=${v}`);
  a.push(file);
  return new Promise((resolve, reject) => {
    const p = spawn('ffmpeg', a, { stdio: ['pipe', 'ignore', 'inherit'] });
    p.on('error', reject);
    p.on('exit', (code) => (code === 0 ? resolve() : reject(new Error(`ffmpeg ${code}: ${file}`))));
    p.stdin.end(Buffer.from(pcm.buffer));
  });
}

// ---------------------------------------------------------------- 앨범

const MOODS = {
  synth: { scale: 'minor', bpm: [84, 100], pad: [1, 0.5, 0.33, 0.25, 0.2, 0.16], arp: 16, rest: 0.1, pluckDecay: 0.35 },
  calm: { scale: 'major', bpm: [64, 76], pad: [1, 0.2, 0.08], arp: 8, rest: 0.25, pluckDecay: 0.6 },
  lofi: { scale: 'dorian', bpm: [72, 84], pad: [1, 0.4, 0.1, 0.05], arp: 8, rest: 0.35, pluckDecay: 0.25 },
  glass: { scale: 'major', bpm: [90, 104], pad: [1, 0, 0.3, 0, 0.15], arp: 16, rest: 0.2, pluckDecay: 0.5 },
  rain: { scale: 'minor', bpm: [56, 64], pad: [1, 0.1], arp: 4, rest: 0.5, pluckDecay: 0.9 },
};

const ALBUMS = [
  {
    dir: 'Neon Harbor/2023 - Night Drive',
    album: 'Night Drive',
    artist: 'Neon Harbor',
    year: 2023,
    genre: 'Synthwave',
    format: 'flac-24-96',
    mood: 'synth',
    cover: 'embed',
    style: 'circles',
    tracks: ['Harbor Lights', 'Overpass at 2 AM', 'Chrome Rain', 'Last Exit'],
  },
  {
    dir: 'Neon Harbor/2019 - Paper Moon',
    album: 'Paper Moon',
    artist: 'Neon Harbor',
    year: 2019,
    genre: 'Synthwave',
    format: 'aiff-16-44',
    mood: 'synth',
    cover: 'folder',
    style: 'rays',
    tracks: ['Paper Moon', 'Static Bloom'],
  },
  {
    dir: '하늘 사중주/아침의 차',
    album: '아침의 차',
    artist: '하늘 사중주',
    year: 2021,
    genre: 'Ambient',
    format: 'flac-16-44',
    mood: 'calm',
    cover: 'folder',
    style: 'waves',
    tracks: ['찻잔 위의 김', '창가의 오후', '느린 산책', '저녁 빛'],
  },
  {
    dir: 'Cassette Club/Lo-Fi Sketches',
    album: 'Lo-Fi Sketches',
    artist: 'Cassette Club',
    year: 2019,
    genre: 'Lo-Fi',
    format: 'mp3-320',
    mood: 'lofi',
    cover: 'embed',
    style: 'grid',
    tracks: ['Tape Hiss Morning', 'Rooftop Notebook', 'B-Side Bus Ride'],
  },
  {
    dir: 'Mira Sol/Glass Garden',
    album: 'Glass Garden',
    artist: 'Mira Sol',
    year: 2022,
    genre: 'Ambient',
    format: 'alac-24-48',
    mood: 'glass',
    cover: 'embed',
    style: 'rays',
    tracks: ['Greenhouse', 'Prism Leaves', 'Dew'],
  },
  {
    dir: 'Field Notes - Rain Studies',
    album: null, // 태그 없음 — 폴더 이름과 파일 이름에서 읽어야 한다
    artist: null,
    year: null,
    genre: null,
    format: 'wav-16-44',
    mood: 'rain',
    cover: 'none',
    tracks: ['01 - Rain on Tin', '02 - Gutter Song'],
  },
  {
    dir: 'Sori Sampler',
    album: 'Sori Sampler',
    artist: 'Various Artists',
    year: 2024,
    genre: 'Electronic',
    format: 'flac-24-48',
    mood: 'glass',
    cover: 'folder',
    style: 'grid',
    discs: [
      [
        ['Neon Harbor', 'Tunnel Vision'],
        ['Mira Sol', 'Sunroom'],
      ],
      [
        ['Cassette Club', 'Rewind'],
        ['하늘 사중주', '밤의 정원'],
      ],
    ],
  },
];

async function main() {
  await fs.mkdir(out, { recursive: true });
  let made = 0;
  for (const a of ALBUMS) {
    const dir = path.join(out, a.dir);
    await fs.mkdir(dir, { recursive: true });
    const seed = seedOf(a.dir);
    let coverFile = null;
    if (a.cover !== 'none') {
      coverFile = path.join(dir, a.cover === 'folder' ? 'cover.jpg' : '.cover-embed.jpg');
      await makeCover(coverFile, seed, a.style);
    }
    const f = FORMATS[a.format];

    const jobs = [];
    if (a.discs) {
      a.discs.forEach((disc, di) => {
        disc.forEach(([artist, title], ti) => {
          jobs.push({
            file: path.join(dir, `CD${di + 1}`, `${String(ti + 1).padStart(2, '0')} ${title}.${f.ext}`),
            tags: {
              title,
              artist,
              album: a.album,
              album_artist: a.artist,
              track: `${ti + 1}/${disc.length}`,
              disc: `${di + 1}/${a.discs.length}`,
              date: a.year,
              genre: a.genre,
            },
          });
        });
      });
    } else {
      a.tracks.forEach((title, ti) => {
        const file = a.album ? `${String(ti + 1).padStart(2, '0')} ${title}.${f.ext}` : `${title}.${f.ext}`;
        jobs.push({
          file: path.join(dir, file),
          tags: a.album
            ? {
                title,
                artist: a.artist,
                album: a.album,
                album_artist: a.artist,
                track: `${ti + 1}/${a.tracks.length}`,
                date: a.year,
                genre: a.genre,
              }
            : {},
        });
      });
    }

    for (const [i, job] of jobs.entries()) {
      await fs.mkdir(path.dirname(job.file), { recursive: true });
      const pcm = synth({ seed: seed + i * 7919, sampleRate: f.sr, seconds: SECONDS - (i % 3) * 4, mood: MOODS[a.mood] });
      await encode(job.file, pcm, a.format, job.tags, a.cover === 'embed' ? coverFile : null);
      made++;
      process.stdout.write(`  ${path.relative(out, job.file)}\n`);
    }
    if (a.cover === 'embed' && coverFile) await fs.rm(coverFile);
  }
  console.log(`\n${made}곡을 ${out}에 만들었습니다.`);
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});
