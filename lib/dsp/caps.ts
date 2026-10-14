/**
 * 이 서버의 ffmpeg로 무엇을 할 수 있는지. 한 번만 묻고 기억한다.
 */
import { spawn } from 'node:child_process';
import { config, hasFfmpeg } from '../config';
import type { Caps } from './plan';

export interface ServerCaps extends Caps {
  ffmpeg: boolean;
  afir: boolean;
  ebur128: boolean;
  version?: string;
}

function run(args: string[]): Promise<{ code: number | null; out: string }> {
  return new Promise((resolve) => {
    try {
      const p = spawn(/* turbopackIgnore: true */ config.ffmpeg, args, { stdio: ['ignore', 'pipe', 'pipe'] });
      let out = '';
      p.stdout.on('data', (d: Buffer) => (out += d.toString()));
      p.stderr.on('data', (d: Buffer) => (out += d.toString()));
      p.on('error', () => resolve({ code: -1, out }));
      p.on('exit', (code) => resolve({ code, out }));
    } catch {
      resolve({ code: -1, out: '' });
    }
  });
}

let caps: Promise<ServerCaps> | null = null;

export function getCaps(): Promise<ServerCaps> {
  caps ??= (async () => {
    if (!(await hasFfmpeg())) return { ffmpeg: false, soxr: false, bs2b: false, afir: false, ebur128: false };
    const [filters, soxr, version] = await Promise.all([
      run(['-hide_banner', '-filters']),
      run([
        '-hide_banner',
        '-v',
        'error',
        '-f',
        'lavfi',
        '-i',
        'anullsrc=r=44100:cl=stereo',
        '-t',
        '0.02',
        '-af',
        'aresample=resampler=soxr:osr=48000',
        '-f',
        'null',
        '-',
      ]),
      run(['-hide_banner', '-version']),
    ]);
    const has = (name: string) => new RegExp(`\\s${name}\\s+[AN|]+->[AN|]+`).test(filters.out);
    return {
      ffmpeg: true,
      soxr: soxr.code === 0,
      bs2b: has('bs2b'),
      afir: has('afir'),
      ebur128: has('ebur128'),
      version: /ffmpeg version (\S+)/.exec(version.out)?.[1],
    };
  })();
  return caps;
}
