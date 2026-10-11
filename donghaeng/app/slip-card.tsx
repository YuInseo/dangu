'use client';

import {
  KIND_LABEL,
  STATUS_LABEL,
  minutesUntil,
  flagLabels,
  needLabels,
  routeLabel,
  seatLabel,
  type Slip,
} from '../lib/slip';

/**
 * 목록의 쪽지 한 장.
 *
 * 종이 쪽지를 훑어볼 때 눈이 가던 순서대로 놓았다: 몇 시, 출발인지 도착인지, 몇 번
 * 열차 몇 번 홈, 몇 호차 어느 자리, 무엇이 필요한지. 연락처와 메모는 펼쳐야 보인다 —
 * 목록에서 그걸 읽을 일은 없고, 지나가는 사람 눈에 띌 이유도 없다.
 */
export function SlipCard({
  slip,
  mine,
  fresh,
  onOpen,
}: {
  slip: Slip;
  mine: boolean;
  fresh: boolean;
  onOpen: (slip: Slip) => void;
}) {
  const left = slip.status === 'done' ? null : minutesUntil(slip);
  const seat = seatLabel(slip);
  const route = routeLabel(slip);
  const needs = needLabels(slip.needs);

  return (
    <button
      className={`card ${slip.kind} ${slip.status}${fresh ? ' fresh' : ''}`}
      onClick={() => onOpen(slip)}
      aria-label={`${KIND_LABEL[slip.kind]} ${slip.time} ${slip.train}열차 ${STATUS_LABEL[slip.status]}`}
    >
      <div className="card-head">
        <span className="kind">{KIND_LABEL[slip.kind]}</span>
        <span className="time">{slip.time || '--:--'}</span>
        {left !== null && <Countdown minutes={left} />}
        <span className={`status ${slip.status}`}>
          {STATUS_LABEL[slip.status]}
          {slip.assignee && slip.status !== 'waiting' && ` · ${mine ? '나' : slip.assignee.name}`}
        </span>
      </div>

      <div className="card-line">
        {slip.train && <span className="train">{slip.train}열차</span>}
        {slip.platform && <span>{slip.platform}번 홈</span>}
        {route && <span>{route}</span>}
        {flagLabels(slip).map((flag) => (
          <span key={flag} className="chip flag">
            {flag}
          </span>
        ))}
      </div>

      {(seat || needs.length > 0) && (
        <div className="card-line">
          {seat && <span className="seat">{seat}</span>}
          {needs.map((need) => (
            <span key={need} className="chip">
              {need}
            </span>
          ))}
        </div>
      )}

      {slip.meet && <div className="card-meet">만날 곳 · {slip.meet}</div>}
    </button>
  );
}

/** 남은 시간. 10분 안쪽이면 눈에 띄게, 지났으면 지났다고. */
function Countdown({ minutes }: { minutes: number }) {
  if (minutes < -1) return <span className="countdown late">{-minutes}분 지남</span>;
  if (minutes <= 0) return <span className="countdown soon">지금</span>;
  const text = minutes >= 60 ? `${Math.floor(minutes / 60)}시간 ${minutes % 60}분 후` : `${minutes}분 후`;
  return <span className={minutes <= 10 ? 'countdown soon' : 'countdown'}>{text}</span>;
}
