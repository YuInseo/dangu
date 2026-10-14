import fs from 'node:fs';
import { Readable } from 'node:stream';
import type { NextRequest } from 'next/server';
import { coverFile, type CoverSize } from '@/lib/library/covers';
import { getLibrary } from '@/lib/library/store';

/** 앨범 표지. URL에 버전이 붙어 있어 한 번 받으면 다시 묻지 않는다. */
export async function GET(req: NextRequest, ctx: { params: Promise<{ id: string }> }) {
  const { id } = await ctx.params;
  const size: CoverSize = req.nextUrl.searchParams.get('s') === 'lg' ? 'lg' : 'sm';
  const lib = await getLibrary();
  const cover = lib.albums.get(id)?.cover;
  if (!cover) return new Response(null, { status: 404 });
  const file = coverFile(id, cover.v, size);
  try {
    const st = await fs.promises.stat(file);
    return new Response(Readable.toWeb(fs.createReadStream(file)) as unknown as ReadableStream<Uint8Array>, {
      headers: {
        'Content-Type': 'image/webp',
        'Content-Length': String(st.size),
        'Cache-Control': 'private, max-age=31536000, immutable',
      },
    });
  } catch {
    return new Response(null, { status: 404 });
  }
}
