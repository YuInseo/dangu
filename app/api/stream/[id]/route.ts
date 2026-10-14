/**
 * 곡을 보낸다.
 *
 * 기본은 디스크의 바이트를 그대로 — 디코딩도, 재인코딩도, 리샘플링도 없다. 브라우저가
 * 받는 것이 파일 그 자체라는 뜻이고, 신호 경로의 "비트퍼펙트"가 가리키는 것이 이것이다.
 *
 * 브라우저가 못 여는 형식(ALAC, DSD, WavPack, APE …)은 `?transcode=1`로 ffmpeg를 거쳐
 * FLAC으로 바꿔 보낸다. 무손실 → 무손실이라 샘플은 그대로이고, DSD만 PCM으로 바뀐다.
 */
import fs from 'node:fs';
import path from 'node:path';
import { spawn } from 'node:child_process';
import { Readable } from 'node:stream';
import type { NextRequest } from 'next/server';
import { config, hasFfmpeg } from '@/lib/config';
import { validSid } from '@/lib/dsp/clips';
import { parseRequest } from '@/lib/dsp/request';
import { dspStream, pipeProcess } from '@/lib/dsp/stream';
import { getLibrary } from '@/lib/library/store';
import { baseMime } from '@/lib/quality';
import type { StoredTrack } from '@/lib/library/types';

function resolveInside(rel: string): string | null {
  const abs = path.resolve(config.musicDir, rel);
  return abs.startsWith(config.musicDir + path.sep) ? abs : null;
}

function toWeb(stream: Readable): ReadableStream<Uint8Array> {
  return Readable.toWeb(stream) as unknown as ReadableStream<Uint8Array>;
}

export async function GET(req: NextRequest, ctx: { params: Promise<{ id: string }> }) {
  const { id } = await ctx.params;
  const lib = await getLibrary();
  const track = lib.tracks.get(id);
  if (!track) return new Response('없는 곡입니다', { status: 404 });
  const abs = resolveInside(track.path);
  if (!abs) return new Response('잘못된 경로', { status: 400 });

  const sp = req.nextUrl.searchParams;
  const at = Math.max(0, Number(sp.get('t')) || 0);

  // DSP: 64비트 체인을 거쳐 FLAC으로. 요청이 이상하면 무시하고 원본으로 간다.
  const dsp = parseRequest(sp.get('dsp'));
  if (dsp) {
    const sid = sp.get('sid');
    return dspStream({
      input: abs,
      req: dsp,
      source: track.format,
      loudness: lib.views.get(id)?.loudness,
      at,
      sid: validSid(sid) ? sid : undefined,
      signal: req.signal,
    });
  }
  if (sp.get('transcode')) return transcode(abs, track, at, req.signal);

  let stat: fs.Stats;
  try {
    stat = await fs.promises.stat(abs);
  } catch {
    return new Response('파일이 사라졌습니다. 라이브러리를 다시 스캔하세요.', { status: 410 });
  }
  const size = stat.size;
  const etag = `"${Math.floor(stat.mtimeMs).toString(36)}-${size.toString(36)}"`;
  const headers: Record<string, string> = {
    'Content-Type': baseMime(track.format.mime),
    'Accept-Ranges': 'bytes',
    'Cache-Control': 'private, max-age=86400',
    ETag: etag,
    'Last-Modified': stat.mtime.toUTCString(),
  };

  const range = req.headers.get('range');
  const m = range ? /^bytes=(\d*)-(\d*)$/.exec(range.trim()) : null;
  // 여러 구간을 한 번에 묻는 요청은 브라우저가 보내지 않는다. 오면 전체를 보낸다.
  if (m && (m[1] || m[2]) && (!req.headers.get('if-range') || req.headers.get('if-range') === etag)) {
    let start: number;
    let end: number;
    if (m[1] === '') {
      start = Math.max(0, size - Number(m[2]));
      end = size - 1;
    } else {
      start = Number(m[1]);
      end = m[2] ? Math.min(Number(m[2]), size - 1) : size - 1;
    }
    if (start >= size || start > end) {
      return new Response(null, { status: 416, headers: { 'Content-Range': `bytes */${size}` } });
    }
    return new Response(toWeb(fs.createReadStream(abs, { start, end })), {
      status: 206,
      headers: { ...headers, 'Content-Range': `bytes ${start}-${end}/${size}`, 'Content-Length': String(end - start + 1) },
    });
  }

  if (req.headers.get('if-none-match') === etag) return new Response(null, { status: 304, headers });
  return new Response(toWeb(fs.createReadStream(abs)), { status: 200, headers: { ...headers, 'Content-Length': String(size) } });
}

async function transcode(abs: string, track: StoredTrack, at: number, signal: AbortSignal) {
  if (!(await hasFfmpeg())) {
    return new Response('이 형식을 재생하려면 서버에 ffmpeg가 필요합니다', { status: 501 });
  }
  const f = track.format;
  const args = ['-hide_banner', '-loglevel', 'error', '-nostdin'];
  if (at > 0) args.push('-ss', at.toFixed(3));
  args.push('-i', abs, '-map', '0:a:0', '-vn', '-c:a', 'flac', '-compression_level', '1');
  // FLAC은 정수 PCM만 담는다. 16비트를 넘거나 부동소수점·DSD면 24비트로.
  if (f.codec === 'DSD' || (f.bitDepth ?? 16) > 16) args.push('-sample_fmt', 's32', '-bits_per_raw_sample', '24');
  // 192kHz를 넘는 것(DSD를 푼 PCM 등)은 브라우저 디코더가 받지 않는 경우가 있다.
  if ((f.sampleRate ?? 0) > 192000 || f.codec === 'DSD') args.push('-ar', '176400');
  args.push('-f', 'flac', 'pipe:1');

  const proc = spawn(/* turbopackIgnore: true */ config.ffmpeg, args, { stdio: ['ignore', 'pipe', 'ignore'] });
  return pipeProcess(proc, signal, { 'X-Sori-Transcoded': 'flac' });
}
