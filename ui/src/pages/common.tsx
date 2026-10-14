import { useEffect, useState } from 'react';
import type { ApiError } from '@/api';
import { CircleAlert } from '@/components/icons';
import { NotFound } from './not-found';

/** 0.3초 넘게 걸릴 때만 보이는 자리표시. 빠르면 깜빡이지 않는다. */
export function Loading() {
  const [show, setShow] = useState(false);
  useEffect(() => {
    const t = setTimeout(() => setShow(true), 300);
    return () => clearTimeout(t);
  }, []);
  return show ? <div className="page-loading" aria-busy="true" /> : null;
}

export function Failed({ error }: { error: ApiError }) {
  if (error.status === 404) return <NotFound />;
  return (
    <div className="notice notice-error">
      <CircleAlert size={18} />
      <div>
        <strong>불러오지 못했습니다.</strong>
        <div className="muted">{error.message}</div>
      </div>
    </div>
  );
}
