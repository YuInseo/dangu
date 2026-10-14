/**
 * Next의 `next/navigation`과 같은 이름. `refresh()`는 화면의 데이터를 다시 받게 한다.
 */
import { useMemo } from 'react';
import { useLocation, useNavigate, useSearchParams as useRouterSearchParams } from 'react-router';
import { refreshData } from '@/api';

export function usePathname(): string {
  return useLocation().pathname;
}

export function useSearchParams(): URLSearchParams {
  return useRouterSearchParams()[0];
}

export function useRouter() {
  const navigate = useNavigate();
  return useMemo(
    () => ({
      push: (href: string, _opts?: { scroll?: boolean }) => navigate(href),
      replace: (href: string, _opts?: { scroll?: boolean }) => navigate(href, { replace: true }),
      back: () => navigate(-1),
      forward: () => navigate(1),
      refresh: () => refreshData(),
      prefetch: (_href: string) => {},
    }),
    [navigate],
  );
}
