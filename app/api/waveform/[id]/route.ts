import { NextResponse } from 'next/server';
import { getLibrary, getWaveform } from '@/lib/library/store';

/** 진행 막대에 그릴 파형. 분석이 끝난 곡만. */
export async function GET(_req: Request, ctx: { params: Promise<{ id: string }> }) {
  const { id } = await ctx.params;
  await getLibrary();
  const w = getWaveform(id);
  if (!w) return NextResponse.json({ error: '아직 분석하지 않았습니다' }, { status: 404, headers: { 'Cache-Control': 'no-store' } });
  return NextResponse.json({ w }, { headers: { 'Cache-Control': 'private, max-age=3600' } });
}
