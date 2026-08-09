'use client';

import Link from 'graft/link';
import { useEffect, useState } from 'react';

import { prepareShell } from '../lib/platform';
import { checkForUpdate, type UpdateCheck } from '../lib/update';

/**
 * 모든 화면 위에 있는 줄: 제목, 업데이트, 설정.
 *
 * 업데이트 버튼이 설정 옆에 있는 이유는 스토어를 거치지 않고 배포하기 때문이다.
 * 새 버전이 있으면 버튼이 초록색 배지로 바뀌므로, 눌러보기 전에 알 수 있다. 확인은
 * 화면이 뜰 때 조용히 한 번만 한다 — 실패해도 아무 말도 하지 않는다. 점수판을 켠
 * 사람이 지금 알고 싶은 건 업데이트가 아니라 점수다.
 */
export function TopBar({ title, back }: { title: string; back?: string }) {
  const [update, setUpdate] = useState<UpdateCheck | null>(null);

  // 상단 줄은 모든 화면에 있으므로, 셸 초기화를 여기서 한 번 부르면 어느 화면으로
  // 들어와도 상태 표시줄이 제자리를 잡는다.
  useEffect(() => {
    void prepareShell();
  }, []);

  useEffect(() => {
    let cancelled = false;
    void checkForUpdate().then((result) => {
      if (!cancelled) setUpdate(result);
    });
    return () => {
      cancelled = true;
    };
  }, []);

  const available = update?.state === 'available';

  return (
    <header className="topbar">
      {back && (
        <Link className="icon-button" href={back} aria-label="뒤로">
          ←
        </Link>
      )}
      <h1>{title}</h1>
      <Link
        className={available ? 'icon-button badge' : 'icon-button'}
        href="/settings#update"
        aria-label={available ? `업데이트 있음 v${update.version}` : '업데이트 확인'}
      >
        {available ? `업데이트 ${update.version}` : '업데이트'}
      </Link>
      <Link className="icon-button" href="/settings" aria-label="설정">
        설정
      </Link>
    </header>
  );
}

// 버전은 설정 → 앱 정보에만 둔다. 첫 화면에 늘 떠 있을 만큼 자주 필요한 값이 아니고,
// 사용자가 물어볼 때 찾아가는 자리가 정해져 있는 편이 낫다.
