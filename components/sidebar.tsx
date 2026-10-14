'use client';

import Link from 'next/link';
import { usePathname } from 'next/navigation';
import type { LibrarySummary, ScanStatus } from '@/lib/library/types';
import { Disc3, House, Library, MicVocal, Radio, Search, Settings, SlidersHorizontal, Tags } from './icons';
import { Logo } from './logo';
import { SidebarScan } from './scan-status';

const LIBRARY = [
  { href: '/', label: '홈', icon: House },
  { href: '/albums', label: '앨범', icon: Disc3 },
  { href: '/artists', label: '아티스트', icon: MicVocal },
  { href: '/tracks', label: '곡', icon: Library },
  { href: '/genres', label: '장르', icon: Tags },
];

function active(pathname: string, href: string) {
  return href === '/' ? pathname === '/' : pathname === href || pathname.startsWith(`${href}/`);
}

export function Sidebar({ status, summary }: { status: ScanStatus; summary: LibrarySummary }) {
  const pathname = usePathname();
  return (
    <nav className="sidebar" aria-label="탐색">
      <Link href="/" className="sidebar-logo" aria-label="Sori 홈">
        <Logo />
      </Link>

      <div className="side-group">
        <div className="side-label">라이브러리</div>
        {LIBRARY.map(({ href, label, icon: Icon }) => (
          <Link key={href} href={href} className={`side-link${active(pathname, href) ? ' is-active' : ''}`}>
            <Icon size={18} />
            {label}
          </Link>
        ))}
      </div>

      <div className="side-group">
        <div className="side-label">오디오</div>
        <Link href="/dsp" className={`side-link${active(pathname, '/dsp') ? ' is-active' : ''}`}>
          <SlidersHorizontal size={18} />
          소리결 DSP
          <span className="side-tag side-tag-blue">64bit</span>
        </Link>
      </div>

      <div className="side-group">
        <div className="side-label">무료 스트리밍</div>
        <Link href="/live" className={`side-link${active(pathname, '/live') ? ' is-active' : ''}`}>
          <Radio size={18} />
          라이브 아카이브
          <span className="side-tag">FLAC</span>
        </Link>
      </div>

      <div className="side-foot">
        <SidebarScan initial={status} initialSummary={summary} />
        <Link href="/settings" className={`side-link${active(pathname, '/settings') ? ' is-active' : ''}`}>
          <Settings size={18} />
          설정
        </Link>
      </div>
    </nav>
  );
}

const MOBILE = [
  { href: '/', label: '홈', icon: House },
  { href: '/albums', label: '앨범', icon: Disc3 },
  { href: '/artists', label: '아티스트', icon: MicVocal },
  { href: '/live', label: '라이브', icon: Radio },
  { href: '/search', label: '검색', icon: Search },
];

export function MobileNav() {
  const pathname = usePathname();
  return (
    <nav className="mobile-nav" aria-label="탐색">
      {MOBILE.map(({ href, label, icon: Icon }) => (
        <Link key={href} href={href} className={`mobile-link${active(pathname, href) ? ' is-active' : ''}`}>
          <Icon size={21} />
          <span>{label}</span>
        </Link>
      ))}
    </nav>
  );
}
