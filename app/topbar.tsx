'use client';

import Link from 'graft/link';
import { useEffect, useState } from 'react';

import { isNativeApp, prepareShell, tap } from '../lib/platform';
import { loadHistory, loadSettings, saveSettings, watchSettings } from '../lib/storage';
import { venueList } from '../lib/stats';
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
export function TopBar({
  title,
  back,
  venues,
}: {
  title: string;
  back?: string;
  /** 제목 자리를 당구장 고르개로 바꾼다. 첫 화면에서만 쓴다. */
  venues?: boolean;
}) {
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
      {venues ? <VenuePicker title={title} /> : <h1>{title}</h1>}
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
      {/* 설정으로 가는 길은 아래 내비게이션이 갖고 있다. 여기 남겨 두면 같은 곳으로 가는
          문이 한 화면에 둘이 된다. 업데이트는 길이 아니라 상태라서 위에 남는다 — 새
          버전이 있는지는 어느 화면에서든 보여야 한다. */}
    </header>
  );
}

/**
 * 제목 자리의 당구장 고르개.
 *
 * 첫 화면에서 제목은 매번 같은 글자다 — 앱 이름을 읽으러 이 화면에 오는 사람은 없다.
 * 그 자리에 "지금 어느 집에 있나"를 놓으면, 그날 처음 앱을 켤 때 한 번 고르는 것으로
 * 그날의 모든 판에 장소가 붙는다.
 *
 * 고른 값은 설정의 `lastVenue`에 남는다. 로비의 장소 칸이 그걸 보고 있으므로 둘은
 * 언제나 같은 것을 가리킨다.
 */
function VenuePicker({ title }: { title: string }) {
  const [open, setOpen] = useState(false);
  const [places, setPlaces] = useState<string[]>([]);
  const [here, setHere] = useState('');

  useEffect(() => {
    let alive = true;
    void (async () => {
      const [settings, history] = await Promise.all([loadSettings(), loadHistory()]);
      if (!alive) return;
      setPlaces(venueList(history, settings.venues ?? []));
      setHere(settings.lastVenue ?? '');
    })();
    // 설정에서 당구장을 더하거나 지우면 이 목록도 따라 바뀐다.
    const stop = watchSettings((settings) => {
      setHere(settings.lastVenue ?? '');
      void loadHistory().then((history) => setPlaces(venueList(history, settings.venues ?? [])));
    });
    return () => {
      alive = false;
      stop();
    };
  }, []);

  const choose = async (name: string) => {
    setHere(name);
    setOpen(false);
    tap();
    const settings = await loadSettings();
    await saveSettings({ ...settings, lastVenue: name });
  };

  // 갈 곳이 없으면 고르개도 없다. 첫 판을 치기 전에는 목록이 비어 있고, 그때 이 자리는
  // 그냥 앱 이름이다.
  if (places.length === 0) return <h1>{title}</h1>;

  return (
    <div className="venue-pick">
      <button className="venue-button" onClick={() => setOpen((current) => !current)}>
        <span>{here || title}</span>
        <span className="caret" aria-hidden="true">
          ▾
        </span>
      </button>

      {open && (
        <>
          {/* 바깥을 누르면 닫힌다. 목록 위에 덮개를 깔지 않으면 다른 버튼을 누르려다
              메뉴만 닫히는 일이 생긴다 — 그건 한 번 더 누르게 만드는 것과 같다. */}
          <button className="venue-veil" aria-label="닫기" onClick={() => setOpen(false)} />
          <div className="venue-menu" role="menu">
            {places.map((name) => (
              <button
                key={name}
                role="menuitem"
                className={name === here ? 'on' : undefined}
                onClick={() => void choose(name)}
              >
                {name}
              </button>
            ))}
            {here && (
              <button role="menuitem" className="clear" onClick={() => void choose('')}>
                장소 없이
              </button>
            )}
          </div>
        </>
      )}
    </div>
  );
}

// 버전은 설정 → 앱 정보에만 둔다. 첫 화면에 늘 떠 있을 만큼 자주 필요한 값이 아니고,
// 사용자가 물어볼 때 찾아가는 자리가 정해져 있는 편이 낫다.
