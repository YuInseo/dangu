'use client';

import Link from 'graft/link';

import { tap } from '../lib/platform';

export type Tab = 'home' | 'calendar' | 'settings';

const TABS: { id: Tab; href: string; label: string; icon: string }[] = [
  { id: 'home', href: '/', label: '쪽지', icon: '▤' },
  { id: 'calendar', href: '/calendar', label: '근무표', icon: '▦' },
  { id: 'settings', href: '/settings', label: '설정', icon: '⚙' },
];

/**
 * 아래 탭. 쪽지판·근무표·설정 사이를 엄지 하나로 오간다.
 *
 * 화면 아래에 붙어 있으므로, 그 위에 뜨는 것(쪽지판의 추가 버튼, 알림)은 `--tabbar-h`만큼
 * 올라가 있어야 한다(globals.css).
 */
export function TabBar({ current }: { current: Tab }) {
  return (
    <nav className="tabbar" aria-label="화면">
      {TABS.map((tab) => (
        <Link
          key={tab.id}
          href={tab.href}
          className={tab.id === current ? 'on' : ''}
          aria-current={tab.id === current ? 'page' : undefined}
          onClick={() => tab.id !== current && tap()}
        >
          <span className="icon" aria-hidden="true">
            {tab.icon}
          </span>
          {tab.label}
        </Link>
      ))}
    </nav>
  );
}
