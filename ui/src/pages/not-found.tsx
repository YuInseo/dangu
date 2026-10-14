import Link from '@/compat/link';
import { useTitle } from '@/api';

export function NotFound() {
  useTitle('찾을 수 없음');
  return (
    <div className="empty">
      <h1>찾는 것이 없어요</h1>
      <p>지워졌거나 라이브러리를 다시 스캔하면서 위치가 바뀌었을 수 있습니다.</p>
      <Link href="/" className="btn">
        홈으로
      </Link>
    </div>
  );
}
