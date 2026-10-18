/**
 * 앱 밖의 주소(유튜브, archive.org …)는 앱의 창이 아니라 기본 브라우저·유튜브 앱에서 연다.
 * 앱 안이면 소리 서버가 앱에 부탁하고(안드로이드: 인텐트, 윈도우: 기본 브라우저), 브라우저로 쓰는
 * 서버면 브라우저가 새 탭으로 연다.
 */
import { useEffect } from 'react';

export async function openExternal(url: string, inApp: boolean) {
  if (inApp) {
    try {
      const res = await fetch('/api/open', {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify({ url }),
      });
      if (res.ok) return;
    } catch {
      // 아래에서 연다
    }
  }
  window.open(url, '_blank', 'noopener,noreferrer');
}

/** `target="_blank"`인 링크를 누르면 앱 밖에서 연다(앱 안에서만). */
export function useExternalLinks(inApp: boolean) {
  useEffect(() => {
    if (!inApp) return;
    const onClick = (e: MouseEvent) => {
      if (e.defaultPrevented || e.button !== 0) return;
      const a = (e.target as Element | null)?.closest?.('a[target="_blank"]') as HTMLAnchorElement | null;
      if (!a || !/^https?:/.test(a.href)) return;
      e.preventDefault();
      void openExternal(a.href, true);
    };
    document.addEventListener('click', onClick);
    return () => document.removeEventListener('click', onClick);
  }, [inApp]);
}
