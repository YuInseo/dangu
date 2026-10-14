import { NextResponse, type NextRequest } from 'next/server';
import { getAnalysisStatus, getLibrary, setAnalysisSpeed } from '@/lib/library/store';

export async function GET() {
  await getLibrary();
  return NextResponse.json(getAnalysisStatus(), { headers: { 'Cache-Control': 'no-store' } });
}

/** 분석 속도 바꾸기: { speed: 'off' | 'throttled' | 'fast' } */
export async function POST(req: NextRequest) {
  const body = (await req.json().catch(() => ({}))) as { speed?: string };
  if (body.speed !== 'off' && body.speed !== 'throttled' && body.speed !== 'fast') {
    return NextResponse.json({ error: '속도는 off, throttled, fast 중 하나' }, { status: 400 });
  }
  return NextResponse.json(await setAnalysisSpeed(body.speed));
}
