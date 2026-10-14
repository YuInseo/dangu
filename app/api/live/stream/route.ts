import type { NextRequest } from 'next/server';
import { validSid } from '@/lib/dsp/clips';
import { parseRequest } from '@/lib/dsp/request';
import { dspStream } from '@/lib/dsp/stream';
import { isValidId } from '@/lib/live/archive';

/**
 * 라이브 아카이브 곡을 DSP 체인에 태워 보낸다. DSP가 꺼져 있으면 브라우저가 archive.org에서
 * 직접 받으므로 이 길을 쓰지 않는다.
 */
export async function GET(req: NextRequest) {
  const sp = req.nextUrl.searchParams;
  const id = sp.get('id') ?? '';
  const file = sp.get('file') ?? '';
  if (!isValidId(id) || !file || file.includes('..') || file.startsWith('/') || file.length > 300) {
    return new Response('잘못된 요청', { status: 400 });
  }
  const dsp = parseRequest(sp.get('dsp'));
  if (!dsp) return new Response('DSP 요청이 없습니다', { status: 400 });
  const ext = file.split('.').pop()?.toLowerCase() ?? '';
  const lossless = ext === 'flac' || ext === 'wav' || ext === 'shn';
  const sid = sp.get('sid');
  const url = `https://archive.org/download/${encodeURIComponent(id)}/${file.split('/').map(encodeURIComponent).join('/')}`;
  return dspStream({
    input: url,
    req: dsp,
    source: { codec: lossless ? 'FLAC' : ext === 'ogg' ? 'Vorbis' : 'MP3', lossless },
    at: Math.max(0, Number(sp.get('t')) || 0),
    sid: validSid(sid) ? sid : undefined,
    signal: req.signal,
  });
}
