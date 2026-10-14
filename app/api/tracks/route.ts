import { NextResponse, type NextRequest } from 'next/server';
import { getLibrary } from '@/lib/library/store';
import { norm } from '@/lib/library/ids';

/** 곡 목록을 나눠서. 수만 곡을 한 번에 브라우저로 보내지 않기 위해서다. */
export async function GET(req: NextRequest) {
  const sp = req.nextUrl.searchParams;
  const offset = Math.max(0, Number(sp.get('offset')) || 0);
  const limit = Math.min(500, Math.max(1, Number(sp.get('limit')) || 200));
  const q = norm(sp.get('q') ?? '');
  const lib = await getLibrary();
  let list = lib.trackList;
  if (q) {
    const terms = q.split(' ');
    list = list.filter((t) => {
      const h = lib.haystack.tracks.get(t.id)!;
      return terms.every((x) => h.includes(x));
    });
  }
  return NextResponse.json({ total: list.length, offset, tracks: list.slice(offset, offset + limit) });
}
