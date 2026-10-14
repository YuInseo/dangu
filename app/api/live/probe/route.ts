import { NextResponse, type NextRequest } from 'next/server';
import { probeFlac } from '@/lib/live/archive';

/** 라이브 아카이브 FLAC의 실제 샘플레이트·비트 깊이. 신호 경로가 묻는다. */
export async function GET(req: NextRequest) {
  const id = req.nextUrl.searchParams.get('id') ?? '';
  const file = req.nextUrl.searchParams.get('file') ?? '';
  try {
    const info = await probeFlac(id, file);
    if (!info) return NextResponse.json({ error: '읽을 수 없습니다' }, { status: 404 });
    return NextResponse.json(info, { headers: { 'Cache-Control': 'private, max-age=86400' } });
  } catch {
    return NextResponse.json({ error: 'archive.org에 닿지 못했습니다' }, { status: 502 });
  }
}
