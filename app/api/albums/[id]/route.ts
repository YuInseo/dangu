import { NextResponse } from 'next/server';
import { getLibrary } from '@/lib/library/store';

/** 앨범 카드의 재생 버튼이 곡 목록을 받으러 오는 곳. */
export async function GET(_req: Request, ctx: { params: Promise<{ id: string }> }) {
  const { id } = await ctx.params;
  const lib = await getLibrary();
  const album = lib.albums.get(id);
  if (!album) return NextResponse.json({ error: '없는 앨범입니다' }, { status: 404 });
  return NextResponse.json({ album, tracks: album.trackIds.map((t) => lib.views.get(t)!) });
}
