'use client';

import Link from 'graft/link';
import { usePathname } from 'graft/navigation';
import { useEffect, useState } from 'react';

import { loadCurrentGame } from '../lib/storage';
import { tap } from '../lib/platform';

/**
 * 화면 아래 세 칸: 점수판, 기록, 설정.
 *
 * 예전에는 이동이 전부 상단 줄에 몰려 있었고, 게임 중에 기록을 보러 갔다 오려면 뒤로
 * 가기와 다시 들어가기를 거쳐야 했다. 당구장에서는 그 왕복이 잦다 — 방금 판이 몇 번째
 * 인지, 오늘 몇 승인지를 치는 중간에 본다.
 *
 * 첫 칸이 상황에 따라 바뀌는 것이 이 줄의 핵심이다. 진행 중인 게임이 있으면 "점수판"이
 * 되어 한 번에 판으로 돌아가고, 없으면 "홈"이 되어 로비로 간다. 게임 중에 기록을 보다가
 * 돌아오는 데 두 번 눌러야 한다면 이 줄을 만든 이유가 없어진다.
 */
export function NavBar() {
  const pathname = usePathname();
  const [playing, setPlaying] = useState(false);

  // 화면을 옮길 때마다 다시 본다. 게임을 시작하거나 끝내고 온 직후에도 첫 칸이 맞아야 한다.
  useEffect(() => {
    let cancelled = false;
    void loadCurrentGame().then((game) => {
      if (!cancelled) setPlaying(Boolean(game && !game.finishedAt));
    });
    return () => {
      cancelled = true;
    };
  }, [pathname]);

  const tabs = [
    playing
      ? { href: '/game', label: '점수판', icon: '🎱' }
      : { href: '/', label: '홈', icon: '🎱' },
    { href: '/stats', label: '기록', icon: '📅' },
    { href: '/settings', label: '설정', icon: '⚙️' },
  ];

  return (
    <nav className="navbar" aria-label="화면 이동">
      {tabs.map((tab) => {
        // `/`는 정확히 같을 때만 켠다. 아니면 모든 화면에서 홈이 함께 켜진다.
        const active = tab.href === '/' ? pathname === '/' : pathname.startsWith(tab.href);
        return (
          <Link
            key={tab.label}
            href={tab.href}
            className={active ? 'tab on' : 'tab'}
            aria-current={active ? 'page' : undefined}
            onClick={() => tap()}
          >
            <span className="icon" aria-hidden="true">
              {tab.icon}
            </span>
            {tab.label}
          </Link>
        );
      })}
    </nav>
  );
}
