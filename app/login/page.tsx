import type { Metadata } from 'next';
import { Lock } from 'lucide-react';
import { LogoMark } from '@/components/logo';
import { safeNext } from '@/lib/auth';

export const metadata: Metadata = { title: '로그인' };

export default async function LoginPage({ searchParams }: { searchParams: Promise<{ next?: string; error?: string }> }) {
  const sp = await searchParams;
  return (
    <main className="login">
      <form className="login-card" action="/api/login" method="post">
        <LogoMark size={44} />
        <h1>Sori</h1>
        <p className="muted">이 서버는 비밀번호로 잠겨 있습니다.</p>
        <input type="hidden" name="next" value={safeNext(sp.next)} />
        <label className="filter filter-lg">
          <Lock size={17} />
          <input
            type="password"
            name="password"
            placeholder="비밀번호"
            autoFocus
            autoComplete="current-password"
            aria-label="비밀번호"
            required
          />
        </label>
        {sp.error && <p className="warn">비밀번호가 맞지 않습니다.</p>}
        <button type="submit" className="btn btn-primary btn-block">
          들어가기
        </button>
      </form>
    </main>
  );
}
