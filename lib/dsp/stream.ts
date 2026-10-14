/**
 * ffmpeg 하나를 띄워 그 출력을 HTTP 응답으로 흘려 보낸다. 변환(transcode)과 DSP가 같이 쓴다.
 */
import { spawn, type ChildProcessByStdio } from 'node:child_process';
import type { Readable } from 'node:stream';
import { config } from '../config';
import { getCaps } from './caps';
import { recordClip, resetClipsAfter } from './clips';
import { getIr } from './ir';
import { planDsp, type SourceInfo } from './plan';
import type { DspRequest, Loudness, WireFilter } from './types';

type Proc = ChildProcessByStdio<null, Readable, Readable | null>;

/** ffmpeg의 표준 출력 → 웹 스트림. 브라우저가 충분히 받았으면 ffmpeg를 잠깐 세운다. */
export function pipeProcess(proc: Proc, signal: AbortSignal, headers: Record<string, string>): Response {
  const kill = () => proc.kill('SIGKILL');
  signal.addEventListener('abort', kill, { once: true });
  proc.on('exit', () => signal.removeEventListener('abort', kill));

  let done = false;
  const body = new ReadableStream<Uint8Array>(
    {
      start(controller) {
        proc.stdout.on('data', (chunk: Buffer) => {
          if (done) return;
          controller.enqueue(new Uint8Array(chunk));
          if ((controller.desiredSize ?? 1) <= 0) proc.stdout.pause();
        });
        proc.stdout.on('end', () => {
          if (done) return;
          done = true;
          controller.close();
        });
        proc.on('error', (e) => {
          if (done) return;
          done = true;
          controller.error(e);
        });
      },
      pull() {
        proc.stdout.resume();
      },
      cancel() {
        done = true;
        kill();
      },
    },
    { highWaterMark: 8 },
  );
  return new Response(body, { status: 200, headers: { 'Content-Type': 'audio/flac', 'Cache-Control': 'no-store', ...headers } });
}

export interface DspStreamOptions {
  /** 파일 경로 또는 http(s) 주소 */
  input: string;
  req: DspRequest;
  source: SourceInfo;
  loudness?: Loudness;
  /** 시작 위치, 초 */
  at: number;
  /** 클리핑 표시기 세션 */
  sid?: string;
  signal: AbortSignal;
}

export async function dspStream(o: DspStreamOptions): Promise<Response> {
  const caps = await getCaps();
  if (!caps.ffmpeg) return new Response('DSP를 쓰려면 서버에 ffmpeg가 필요합니다', { status: 501 });

  // 임펄스 응답이 사라졌거나 afir가 없는 ffmpeg면 그 필터만 빼고 나머지는 그대로 건다.
  const irs = new Map<string, { file: string; channels: number }>();
  const filters: WireFilter[] = [];
  for (const f of o.req.f) {
    if (f.t === 'convolution') {
      if (!caps.afir) continue;
      const info = await getIr(f.ir);
      if (!info) continue;
      irs.set(f.ir, { file: info.file, channels: info.channels });
    }
    filters.push(f);
  }
  const req = { ...o.req, f: filters };
  const plan = planDsp(req, o.source, o.loudness, { caps, irMono: (id) => irs.get(id)?.channels === 1 });
  const clip = plan.clip && !!o.sid;

  const args = ['-hide_banner', '-nostdin', '-nostats', '-loglevel', clip ? 'info' : 'error'];
  if (o.at > 0) args.push('-ss', o.at.toFixed(3));
  args.push('-i', o.input);
  for (const ir of plan.irs) args.push('-i', irs.get(ir.id)!.file);
  args.push('-filter_complex', plan.graph, '-map', '[out]', '-c:a', 'flac', '-compression_level', '1');
  if (plan.outBits === 24) args.push('-sample_fmt', 's32', '-bits_per_raw_sample', '24');
  else args.push('-sample_fmt', 's16');
  args.push('-f', 'flac', 'pipe:1');

  const proc = spawn(/* turbopackIgnore: true */ config.ffmpeg, args, { stdio: ['ignore', 'pipe', clip ? 'pipe' : 'ignore'] });

  if (clip && proc.stderr) {
    const sid = o.sid!;
    resetClipsAfter(sid, o.at);
    let buf = '';
    let pts = 0;
    proc.stderr.on('data', (d: Buffer) => {
      buf += d.toString();
      let nl: number;
      while ((nl = buf.indexOf('\n')) >= 0) {
        const line = buf.slice(0, nl);
        buf = buf.slice(nl + 1);
        const t = /pts_time:([\d.]+)/.exec(line);
        if (t) pts = Number(t[1]);
        const p = /Overall\.Peak_level=([-\d.]+)/.exec(line);
        if (p) recordClip(sid, o.at + pts, Number(p[1]));
      }
      if (buf.length > 1 << 16) buf = '';
    });
  }

  return pipeProcess(proc as Proc, o.signal, {
    'X-Sori-Dsp': '1',
    'X-Sori-Rate': String(plan.outRate ?? ''),
    'X-Sori-Bits': String(plan.outBits),
  });
}
