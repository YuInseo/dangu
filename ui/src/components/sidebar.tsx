'use client';

import Link from '@/compat/link';
import { usePathname } from '@/compat/navigation';
import { useApi } from '@/api';
import type { PlaylistSummary } from '@/lib/collections';
import type { LibrarySummary, ScanStatus } from '@/lib/library/types';
import { Disc3, Heart, House, Library, LibraryBig, ListMusic, MicVocal, Radio, Settings, SlidersHorizontal, Tags } from './icons';
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
        <div className="side-label">보관함</div>
        <Link href="/favorites" className={`side-link${active(pathname, '/favorites') ? ' is-active' : ''}`}>
          <Heart size={18} />
          좋아요
        </Link>
        <Link href="/playlists" className={`side-link${pathname === '/playlists' ? ' is-active' : ''}`}>
          <ListMusic size={18} />
          플레이리스트
        </Link>
        <SidePlaylists pathname={pathname} />
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

/** 최근에 고친 플레이리스트 몇 개를 바로 갈 수 있게 */
function SidePlaylists({ pathname }: { pathname: string }) {
  const { data } = useApi<PlaylistSummary[]>('/api/playlists');
  if (!data?.length) return null;
  return (
    <div className="side-sub">
      {data.slice(0, 8).map((p) => (
        <Link
          key={p.id}
          href={`/playlists/${p.id}`}
          className={`side-sub-link${pathname === `/playlists/${p.id}` ? ' is-active' : ''}`}
          title={p.name}
        >
          {p.name}
        </Link>
      ))}
    </div>
  );
}

/**
 * 휴대폰 아래쪽 탭. 아티스트·곡·장르·좋아요·플레이리스트는 보관함 안에, 검색은 위쪽 검색창에 있다.
 * 설정(음악 폴더, DSP, 연결, 업데이트)은 옆 막대가 없는 휴대폰에서 여기로 간다.
 */
const MOBILE = [
  { href: '/', label: '홈', icon: House, also: [] as string[] },
  { href: '/albums', label: '앨범', icon: Disc3, also: [] },
  { href: '/collection', label: '보관함', icon: LibraryBig, also: ['/favorites', '/playlists', '/artists', '/tracks', '/genres'] },
  { href: '/live', label: '라이브', icon: Radio, also: [] },
  { href: '/settings', label: '설정', icon: Settings, also: ['/dsp'] },
];

export function MobileNav() {
  const pathname = usePathname();
  return (
    <nav className="mobile-nav" aria-label="탐색">
      {MOBILE.map(({ href, label, icon: Icon, also }) => (
        <Link
          key={href}
          href={href}
          className={`mobile-link${active(pathname, href) || also.some((a) => active(pathname, a)) ? ' is-active' : ''}`}
        >
          <Icon size={21} />
          <span>{label}</span>
        </Link>
      ))}
    </nav>
  );
}
