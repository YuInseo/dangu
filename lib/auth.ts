/**
 * 비밀번호 하나짜리 잠금.
 *
 * 쿠키에는 비밀번호가 아니라 비밀번호로 서명한 고정 문자열이 들어간다. 비밀번호를
 * 바꾸면 모든 기기가 다시 로그인하게 된다.
 */
import { createHmac, timingSafeEqual } from 'node:crypto';

export const SESSION_COOKIE = 'sori_session';

export function sessionToken(password: string): string {
  return createHmac('sha256', password).update('sori-session-v1').digest('base64url');
}

export function isValidSession(password: string, value: string | undefined): boolean {
  if (!value) return false;
  const a = Buffer.from(sessionToken(password));
  const b = Buffer.from(value);
  return a.length === b.length && timingSafeEqual(a, b);
}

export function passwordMatches(password: string, input: string): boolean {
  const a = createHmac('sha256', 'sori').update(password).digest();
  const b = createHmac('sha256', 'sori').update(input).digest();
  return timingSafeEqual(a, b);
}

/** 로그인 뒤 돌아갈 곳. 바깥 주소로 튕기는 것을 막는다. */
export function safeNext(next: string | null | undefined): string {
  if (!next || !next.startsWith('/') || next.startsWith('//') || next.startsWith('/\\')) return '/';
  return next;
}
