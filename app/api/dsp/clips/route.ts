import { NextResponse, type NextRequest } from 'next/server';
import { getClips, validSid } from '@/lib/dsp/clips';

/** 클리핑이 난 곡 안의 시각들. 브라우저가 재생 위치와 맞춰 표시등을 켠다. */
export async function GET(req: NextRequest) {
  const sid = req.nextUrl.searchParams.get('sid');
  if (!validSid(sid)) return NextResponse.json({ error: '잘못된 세션' }, { status: 400 });
  return NextResponse.json(getClips(sid), { headers: { 'Cache-Control': 'no-store' } });
}
