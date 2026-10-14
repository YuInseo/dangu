'use client';

import { useRouter } from '@/compat/navigation';
import { useEffect, useRef, useState } from 'react';
import type { LibrarySummary, ScanStatus } from '@/lib/library/types';
import { count } from '@/lib/time';
import { CircleAlert, LoaderCircle, RefreshCw } from './icons';

/** 스캔 상태를 따라간다. 끝나는 순간 화면을 새로 그린다. */
export function useScanStatus(initial: ScanStatus) {
  const [status, setStatus] = useState(initial);
  const [summary, setSummary] = useState<LibrarySummary | null>(null);
  const router = useRouter();
  const was = useRef(initial.state);

  useEffect(() => {
    let alive = true;
    let timer: ReturnType<typeof setTimeout>;
    const tick = async () => {
      try {
        const res = await fetch('/api/scan', { cache: 'no-store' });
        if (res.ok) {
          const json = (await res.json()) as { status: ScanStatus; summary: LibrarySummary };
          if (!alive) return;
          setStatus(json.status);
          setSummary(json.summary);
          if (was.current === 'scanning' && json.status.state !== 'scanning') router.refresh();
          was.current = json.status.state;
        }
      } catch {
        // 서버가 잠깐 없을 수 있다
      }
      if (alive) timer = setTimeout(tick, was.current === 'scanning' ? 1200 : 15000);
    };
    timer = setTimeout(tick, initial.state === 'scanning' ? 600 : 3000);
    return () => {
      alive = false;
      clearTimeout(timer);
    };
  }, [initial.state, router]);

  const rescan = async () => {
    const res = await fetch('/api/scan', { method: 'POST' });
    if (res.ok) {
      const json = (await res.json()) as { status: ScanStatus };
      setStatus(json.status);
      was.current = json.status.state;
      // 폴링 주기를 당긴다
      setTimeout(async () => {
        const r = await fetch('/api/scan', { cache: 'no-store' }).catch(() => null);
        if (r?.ok) {
          const j = (await r.json()) as { status: ScanStatus };
          setStatus(j.status);
          if (j.status.state !== 'scanning') router.refresh();
          was.current = j.status.state;
        }
      }, 800);
    }
  };

  return { status, summary, rescan };
}

const PHASE: Record<NonNullable<ScanStatus['phase']>, string> = {
  listing: '파일 찾는 중',
  reading: '태그 읽는 중',
  covers: '표지 만드는 중',
  saving: '저장 중',
};

export function scanText(s: ScanStatus): string {
  if (s.state === 'scanning') {
    const phase = s.phase ? PHASE[s.phase] : '스캔 중';
    return s.total ? `${phase} · ${count(s.processed)}/${count(s.total)}` : phase;
  }
  if (s.state === 'error') return s.error ?? '스캔하지 못했습니다';
  return '';
}

export function SidebarScan({ initial, initialSummary }: { initial: ScanStatus; initialSummary: LibrarySummary }) {
  const { status, summary, rescan } = useScanStatus(initial);
  const sum = summary ?? initialSummary;
  return (
    <div className={`side-scan side-scan-${status.state}`}>
      {status.state === 'scanning' ? (
        <>
          <LoaderCircle size={14} className="spin" />
          <span>{scanText(status)}</span>
        </>
      ) : status.state === 'error' ? (
        <>
          <CircleAlert size={14} />
          <span title={status.error}>음악 폴더를 읽지 못했습니다</span>
        </>
      ) : (
        <>
          <span>
            앨범 {count(sum.albums)} · 곡 {count(sum.tracks)}
          </span>
          <button type="button" className="icon-btn icon-btn-xs" onClick={rescan} aria-label="다시 스캔" title="다시 스캔">
            <RefreshCw size={13} />
          </button>
        </>
      )}
    </div>
  );
}
