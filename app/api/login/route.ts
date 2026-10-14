import { NextResponse, type NextRequest } from 'next/server';
import { SESSION_COOKIE, passwordMatches, safeNext, sessionToken } from '@/lib/auth';

/** 자바스크립트 없이도 되는 평범한 폼 제출. */
export async function POST(req: NextRequest) {
  const password = process.env.SORI_PASSWORD ?? '';
  const form = await req.formData();
  const input = String(form.get('password') ?? '');
  const next = safeNext(String(form.get('next') ?? '/'));

  if (!password || !passwordMatches(password, input)) {
    const url = new URL('/login', req.url);
    url.searchParams.set('error', '1');
    url.searchParams.set('next', next);
    return NextResponse.redirect(url, 303);
  }
  const res = NextResponse.redirect(new URL(next, req.url), 303);
  res.cookies.set(SESSION_COOKIE, sessionToken(password), {
    httpOnly: true,
    sameSite: 'lax',
    secure: req.nextUrl.protocol === 'https:',
    path: '/',
    maxAge: 60 * 60 * 24 * 365,
  });
  return res;
}
