import { NextResponse, type NextRequest } from 'next/server';
import { SESSION_COOKIE, isValidSession } from './lib/auth';

/**
 * `SORI_PASSWORD`가 있으면 로그인한 사람만 들어온다. 없으면 아무것도 하지 않는다.
 */
export function proxy(req: NextRequest) {
  const password = process.env.SORI_PASSWORD;
  if (!password) return NextResponse.next();

  const { pathname, search } = req.nextUrl;
  if (pathname === '/login' || pathname === '/api/login') return NextResponse.next();
  if (isValidSession(password, req.cookies.get(SESSION_COOKIE)?.value)) return NextResponse.next();

  if (pathname.startsWith('/api/')) {
    return NextResponse.json({ error: '로그인이 필요합니다' }, { status: 401 });
  }
  const url = req.nextUrl.clone();
  url.pathname = '/login';
  url.search = `?next=${encodeURIComponent(pathname + search)}`;
  return NextResponse.redirect(url);
}

export const config = {
  matcher: ['/((?!_next/static|_next/image|favicon.ico|icon.svg|manifest.webmanifest).*)'],
};
