import { NextResponse } from 'next/server';
import { getLibrary, getScanStatus, startScan } from '@/lib/library/store';

export async function GET() {
  const lib = await getLibrary();
  return NextResponse.json({ status: getScanStatus(), summary: lib.summary }, { headers: { 'Cache-Control': 'no-store' } });
}

/** 다시 스캔. 이미 도는 중이면 그 상태를 돌려준다. */
export async function POST() {
  await getLibrary();
  return NextResponse.json({ status: startScan() });
}
