import type { AudioFormat } from '@/lib/library/types';
import { formatLabel, tierOf, TIER_LABEL } from '@/lib/quality';

/** "Hi-Res · FLAC 24/96" 같은 작은 배지. */
export function QualityBadge({ format, compact }: { format: AudioFormat; compact?: boolean }) {
  const tier = tierOf(format);
  return (
    <span className={`qbadge qbadge-${tier}`} title={`${TIER_LABEL[tier]} · ${formatLabel(format)}`}>
      {!compact && <span className="qbadge-tier">{TIER_LABEL[tier]}</span>}
      <span className="qbadge-fmt">{formatLabel(format)}</span>
    </span>
  );
}

/** 표지 귀퉁이에 붙이는 Hi-Res 표시. 무손실·손실에는 붙이지 않는다 — 다 붙으면 아무것도 말하지 않는다. */
export function HiResMark({ format }: { format: AudioFormat }) {
  if (tierOf(format) !== 'hires') return null;
  return <span className="hires-mark">Hi-Res</span>;
}
