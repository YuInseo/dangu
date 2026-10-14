'use client';

import type { ScanStatus } from '@/lib/library/types';
import { scanText, useScanStatus } from './scan-status';

export function LiveScanProgress({ initial }: { initial: ScanStatus }) {
  const { status } = useScanStatus(initial);
  const pct = status.total ? Math.round((status.processed / status.total) * 100) : 0;
  return (
    <div className="scan-progress">
      <div className="bar">
        <span style={{ width: `${status.state === 'scanning' ? pct : 100}%` }} />
      </div>
      <div className="muted">{status.state === 'scanning' ? scanText(status) : '끝났습니다'}</div>
    </div>
  );
}
