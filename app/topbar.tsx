'use client';

import Link from 'graft/link';
import { useEffect, useState } from 'react';

import { isNativeApp, prepareShell } from '../lib/platform';
import { confirmBundle, stageWebBundle } from '../lib/live-update';
import { checkForUpdate, needsApk, prefetchApk, type UpdateCheck } from '../lib/update';

/**
 * 모든 화면 위에 있는 줄: 제목, 업데이트, 설정.
 *
 * 업데이트는 대개 여기서 조용히 끝난다. 새 웹 번들이 있으면 아무 말 없이 받아 두고,
 * 다음에 앱을 켤 때 새 화면이 뜬다 — 누를 것도, 기다릴 것도 없다. 배지가 켜지는 건
 * 앱 껍데기까지 바뀌어서 안드로이드 설치 화면을 한 번 거쳐야 할 때뿐이다. 그때도
 * APK는 이미 받아 둔 뒤라 누르면 곧바로 설치 화면이다.
 *
 * 실패는 전부 조용하다. 점수판을 켠 사람이 지금 알고 싶은 건 업데이트가 아니라 점수다.
 */
export function TopBar({ title, back }: { title: string; back?: string }) {
  const [update, setUpdate] = useState<UpdateCheck | null>(null);
  const [apk, setApk] = useState(false);
  /** 받아 두었고 다음 실행에 적용될 버전. 있으면 "다시 켜면 됩니다"를 띄운다. */
  const [ready, setReady] = useState<string | null>(null);

  // 상단 줄은 모든 화면에 있으므로, 셸 초기화를 여기서 한 번 부르면 어느 화면으로
  // 들어와도 상태 표시줄이 제자리를 잡는다.
  useEffect(() => {
    void prepareShell();
    // 이 실행이 멀쩡하다는 신호. 늦으면 새 번들이 되돌려지므로 다른 무엇보다 먼저 보낸다.
    void confirmBundle();
  }, []);

  useEffect(() => {
    let cancelled = false;

    void (async () => {
      const result = await checkForUpdate();
      if (cancelled) return;
      setUpdate(result);
      if (result.state !== 'available') return;

      // 화면은 조용히 갈아끼운다. 다음 실행에 적용되므로 지금 치는 게임은 건드리지 않는다.
      //
      // 받아 둔 뒤에는 그 사실을 알려 준다. 조용한 것과 아무 말도 안 하는 것은 다르다 —
      // 새 버전이 왔는데 화면이 그대로면, 다시 켜면 된다는 걸 알 방법이 없다.
      void stageWebBundle(result.release).then((staged) => {
        if (!cancelled && staged.state === 'ready') setReady(staged.version);
      });

      // 껍데기까지 바뀐 릴리스에서만 APK를 미리 받는다. 4MB는 당구장 3G에서 공짜가 아니다.
      const native = await needsApk(result.release);
      if (cancelled || !native) return;
      setApk(true);
      void prefetchApk(result.release);
    })();

    return () => {
      cancelled = true;
    };
  }, []);

  // 웹에서는 갈아끼울 껍데기가 없으므로, 새 버전이 있다는 사실 자체가 알릴 거리다.
  const available = update?.state === 'available' && (apk || !isNativeApp());

  // 받아 둔 것이 있으면 그게 먼저다. 사용자가 할 일이 "설치"가 아니라 "다시 켜기"라는
  // 것을 알려 주는 쪽이, 새 버전이 있다는 사실보다 쓸모 있다.
  const label = ready ? `재시작 ${ready}` : available ? `업데이트 ${update.version}` : '업데이트';
  const lit = Boolean(ready) || available;

  return (
    <header className="topbar">
      {back && (
        <Link className="icon-button" href={back} aria-label="뒤로">
          ←
        </Link>
      )}
      <h1>{title}</h1>
      <Link
        className={lit ? 'icon-button badge' : 'icon-button'}
        href="/settings#update"
        aria-label={
          ready
            ? `v${ready} 준비됨. 앱을 다시 켜면 적용됩니다`
            : available
              ? `업데이트 있음 v${update.version}`
              : '업데이트 확인'
        }
      >
        {label}
      </Link>
      <Link className="icon-button" href="/settings" aria-label="설정">
        설정
      </Link>
    </header>
  );
}

// 버전은 설정 → 앱 정보에만 둔다. 첫 화면에 늘 떠 있을 만큼 자주 필요한 값이 아니고,
// 사용자가 물어볼 때 찾아가는 자리가 정해져 있는 편이 낫다.
