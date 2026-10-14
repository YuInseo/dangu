import { Lock } from 'lucide-react';
import { useSearchParams } from '@/compat/navigation';
import { useTitle } from '@/api';
import { LogoMark } from '@/components/logo';

function safeNext(next: string | null): string {
  if (!next || !next.startsWith('/') || next.startsWith('//') || next.startsWith('/\\')) return '/';
  return next;
}

export function LoginPage() {
  useTitle('로그인');
  const sp = useSearchParams();
  return (
    <main className="login">
      <form className="login-card" action="/api/login" method="post">
        <LogoMark size={44} />
        <h1>Sori</h1>
        <p className="muted">이 서버는 비밀번호로 잠겨 있습니다.</p>
        <input type="hidden" name="next" value={safeNext(sp.get('next'))} />
        <label className="filter filter-lg">
          <Lock size={17} />
          <input type="password" name="password" placeholder="비밀번호" autoFocus autoComplete="current-password" aria-label="비밀번호" required />
        </label>
        {sp.get('error') && <p className="warn">비밀번호가 맞지 않습니다.</p>}
        <button type="submit" className="btn btn-primary btn-block">
          들어가기
        </button>
      </form>
    </main>
  );
}
