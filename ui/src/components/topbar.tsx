'use client';

import Link from '@/compat/link';
import { usePathname, useRouter, useSearchParams } from '@/compat/navigation';
import { useEffect, useRef, useState } from 'react';
import { ChevronLeft, ChevronRight, Search, Settings, X } from './icons';

/** 위쪽 띠. 뒤로·앞으로, 그리고 치는 대로 찾는 검색창. */
export function Topbar() {
  const router = useRouter();
  const pathname = usePathname();
  const params = useSearchParams();
  const onSearch = pathname === '/search';
  const [q, setQ] = useState(onSearch ? (params.get('q') ?? '') : '');
  const input = useRef<HTMLInputElement>(null);
  const timer = useRef<ReturnType<typeof setTimeout>>(undefined);
  const [scrolled, setScrolled] = useState(false);

  useEffect(() => {
    if (!onSearch) setQ('');
  }, [onSearch]);

  useEffect(() => {
    const onScroll = () => setScrolled(window.scrollY > 8);
    onScroll();
    window.addEventListener('scroll', onScroll, { passive: true });
    return () => window.removeEventListener('scroll', onScroll);
  }, []);

  useEffect(() => {
    const focus = () => {
      input.current?.focus();
      input.current?.select();
    };
    window.addEventListener('sori:focus-search', focus);
    return () => window.removeEventListener('sori:focus-search', focus);
  }, []);

  const go = (value: string) => {
    clearTimeout(timer.current);
    timer.current = setTimeout(() => {
      const url = value.trim() ? `/search?q=${encodeURIComponent(value.trim())}` : '/search';
      if (onSearch) router.replace(url, { scroll: false });
      else router.push(url);
    }, 220);
  };

  return (
    <div className={`topbar${scrolled ? ' is-scrolled' : ''}`}>
      <div className="topbar-nav">
        <button type="button" className="icon-btn round" onClick={() => router.back()} aria-label="뒤로">
          <ChevronLeft size={20} />
        </button>
        <button type="button" className="icon-btn round" onClick={() => router.forward()} aria-label="앞으로">
          <ChevronRight size={20} />
        </button>
      </div>
      <form
        className="search"
        role="search"
        onSubmit={(e) => {
          e.preventDefault();
          go(q);
        }}
      >
        <Search size={17} className="search-icon" />
        <input
          ref={input}
          type="search"
          value={q}
          placeholder="아티스트, 앨범, 곡 찾기"
          aria-label="검색"
          onChange={(e) => {
            setQ(e.target.value);
            go(e.target.value);
          }}
        />
        {q && (
          <button
            type="button"
            className="search-clear"
            aria-label="지우기"
            onClick={() => {
              setQ('');
              go('');
              input.current?.focus();
            }}
          >
            <X size={15} />
          </button>
        )}
        <kbd className="search-kbd">/</kbd>
      </form>
      {/* 휴대폰: 옆 막대가 없으니 설정(음악 폴더, DSP, 연결)으로 가는 길을 여기에 둔다. */}
      <Link
        href="/settings"
        className={`icon-btn round topbar-settings${pathname.startsWith('/settings') ? ' is-on' : ''}`}
        aria-label="설정"
      >
        <Settings size={20} />
      </Link>
    </div>
  );
}
