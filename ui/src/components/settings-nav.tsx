'use client';

import Link from '@/compat/link';
import { usePathname } from '@/compat/navigation';

const ITEMS = [
  { href: '/settings', label: '일반' },
  { href: '/settings/play', label: '재생 동작' },
  { href: '/settings/library', label: '라이브러리' },
  { href: '/settings/audio', label: '오디오' },
  { href: '/settings/about', label: '정보' },
];

export function SettingsNav() {
  const pathname = usePathname();
  return (
    <nav className="settings-nav" aria-label="설정">
      <h1 className="settings-title">설정</h1>
      {ITEMS.map((i) => (
        <Link key={i.href} href={i.href} className={`settings-link${pathname === i.href ? ' is-active' : ''}`} replace scroll={false}>
          {i.label}
        </Link>
      ))}
    </nav>
  );
}
