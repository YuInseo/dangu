import { KeyRound, Lock } from 'lucide-react';
import { useEffect, useState } from 'react';
import { useSearchParams } from '@/compat/navigation';
import type { Hello } from '@/lib/link';
import { useTitle } from '@/api';
import { LogoMark } from '@/components/logo';

function safeNext(next: string | null): string {
  if (!next || !next.startsWith('/') || next.startsWith('//') || next.startsWith('/\\')) return '/';
  return next;
}

export function LoginPage() {
  useTitle('로그인');
  const sp = useSearchParams();
  // 같은 와이파이로 연 기기는 6자리 PIN으로 잠겨 있다.
  const [hello, setHello] = useState<Hello | null>(null);
  useEffect(() => {
    fetch('/api/hello', { cache: 'no-store' })
      .then((r) => (r.ok ? r.json() : null))
      .then((j: Hello | null) => setHello(j))
      .catch(() => {});
  }, []);
  const pin = !!hello?.pin;
  const what = pin ? 'PIN' : '비밀번호';
  return (
    <main className="login">
      <form className="login-card" action="/api/login" method="post">
        <LogoMark size={44} />
        <h1>Sori</h1>
        <p className="muted">
          {pin ? `${hello?.name ?? '그 기기'}의 설정 · 연결에 보이는 6자리 PIN을 넣으세요.` : '이 서버는 비밀번호로 잠겨 있습니다.'}
        </p>
        <input type="hidden" name="next" value={safeNext(sp.get('next'))} />
        <label className="filter filter-lg">
          {pin ? <KeyRound size={17} /> : <Lock size={17} />}
          <input
            type="password"
            name="password"
            placeholder={what}
            autoFocus
            autoComplete={pin ? 'one-time-code' : 'current-password'}
            inputMode={pin ? 'numeric' : undefined}
            aria-label={what}
            required
          />
        </label>
        {sp.get('error') && <p className="warn">{what}가 맞지 않습니다.</p>}
        <button type="submit" className="btn btn-primary btn-block">
          들어가기
        </button>
      </form>
    </main>
  );
}
