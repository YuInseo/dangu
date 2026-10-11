'use client';

import { useEffect, useState } from 'react';

import { tap } from '../lib/platform';
import { applyNow, openApk, prepareUpdate, type UpdatePlan } from '../lib/update';

/**
 * 업데이트 알림 줄.
 *
 * 대개는 아무것도 보이지 않는다. 새 화면을 받아 두었으면 "다시 켜면 적용"을 한 줄로
 * 알리고(조용한 것과 아무 말도 안 하는 것은 다르다), 껍데기까지 바뀌었으면 설치 버튼을
 * 띄운다. 닫으면 이번 실행 동안은 다시 뜨지 않는다.
 */
export function UpdateBanner() {
  const [plan, setPlan] = useState<UpdatePlan>({ kind: 'none' });
  const [hidden, setHidden] = useState(false);

  useEffect(() => {
    let alive = true;
    // 켜자마자 묻지 않는다. 첫 화면이 뜨는 데 쓸 회선을 업데이트가 먹지 않게.
    const timer = setTimeout(() => {
      void prepareUpdate().then((next) => alive && setPlan(next));
    }, 3000);
    return () => {
      alive = false;
      clearTimeout(timer);
    };
  }, []);

  if (hidden || plan.kind === 'none') return null;

  return (
    <div className="update-banner" role="status">
      {plan.kind === 'staged' ? (
        <>
          <span>새 버전 {plan.release.version} 준비됨 · 다시 켜면 적용</span>
          <button
            className="primary"
            onClick={() => {
              tap();
              void applyNow(plan.bundleId);
            }}
          >
            지금 적용
          </button>
        </>
      ) : (
        <>
          <span>새 앱 {plan.release.version} 설치가 필요합니다</span>
          <button
            className="primary"
            onClick={() => {
              tap();
              openApk(plan.release);
            }}
          >
            받기
          </button>
        </>
      )}
      <button className="ghost close" aria-label="닫기" onClick={() => setHidden(true)}>
        ✕
      </button>
    </div>
  );
}
