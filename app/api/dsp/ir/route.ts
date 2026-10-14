import { NextResponse, type NextRequest } from 'next/server';
import { deleteIr, IR_MAX_BYTES, IrError, listIrs, saveIr } from '@/lib/dsp/ir';

/** 컨볼루션 임펄스 응답: 목록, 올리기, 지우기. */
export async function GET() {
  return NextResponse.json({ irs: await listIrs() });
}

export async function POST(req: NextRequest) {
  const len = Number(req.headers.get('content-length') ?? 0);
  if (len > IR_MAX_BYTES + 1024 * 1024) return NextResponse.json({ error: '50MB보다 큰 파일은 받지 않습니다' }, { status: 413 });
  let file: File | null = null;
  try {
    const form = await req.formData();
    const f = form.get('file');
    if (f instanceof File) file = f;
  } catch {
    // 아래에서 처리
  }
  if (!file) return NextResponse.json({ error: '파일이 없습니다' }, { status: 400 });
  try {
    const info = await saveIr(file.name, new Uint8Array(await file.arrayBuffer()));
    return NextResponse.json({ ir: info });
  } catch (e) {
    const message = e instanceof IrError ? e.message : '저장하지 못했습니다';
    return NextResponse.json({ error: message }, { status: 400 });
  }
}

export async function DELETE(req: NextRequest) {
  const id = req.nextUrl.searchParams.get('id') ?? '';
  return (await deleteIr(id)) ? NextResponse.json({ ok: true }) : NextResponse.json({ error: '없는 파일입니다' }, { status: 404 });
}
