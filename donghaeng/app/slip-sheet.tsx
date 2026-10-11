'use client';

import { useEffect, useState } from 'react';

import { deleteSlip, setCheck, setStatus, type Team } from '../lib/firebase';
import { shareText, tap } from '../lib/platform';
import {
  CHECKS,
  KIND_LABEL,
  NEEDS,
  STATUS_LABEL,
  flagLabels,
  formatPhone,
  routeLabel,
  seatLabel,
  shareText as slipText,
  telHref,
  type Person,
  type Slip,
} from '../lib/slip';

/**
 * 쪽지를 펼친 것.
 *
 * 맨 아래 큰 버튼 하나가 지금 할 일이다: 대기면 "내가 맡기", 진행이면 "완료". 맡은 사람이
 * 따로 정해져 있던 종이 시절에는 쪽지를 손에 쥔 사람이 곧 담당이었다. 여기서는 그 "쥐는"
 * 동작이 이 버튼이고, 누르는 순간 팀 모두의 화면에 이름이 뜬다 — 두 사람이 같은 고객을
 * 모시러 가는 일이 없어진다.
 */
export function SlipSheet({
  slip,
  team,
  me,
  onClose,
  onEdit,
  onNotice,
}: {
  slip: Slip;
  team: Team;
  me: Person;
  onClose: () => void;
  onEdit: () => void;
  onNotice: (message: string) => void;
}) {
  const [confirming, setConfirming] = useState(false);

  // 뒤로 가기(안드로이드)와 Esc로 닫힌다.
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => event.key === 'Escape' && onClose();
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [onClose]);

  const tel = telHref(slip.phone);
  const mine = slip.assignee?.uid === me.uid;
  const takenByOther = slip.status === 'doing' && slip.assignee && !mine;

  const change = (status: Slip['status']) => {
    tap('medium');
    void setStatus(team.code, slip, status, me);
    if (status === 'done') onClose();
  };

  const share = async () => {
    tap();
    const result = await shareText(`${KIND_LABEL[slip.kind]} 쪽지`, slipText(slip));
    if (result === 'copied') onNotice('쪽지 내용을 복사했습니다. 메신저에 붙여넣으세요.');
    if (result === 'failed') onNotice('공유하지 못했습니다.');
  };

  // 완료된 쪽지는 기록이다. 누가 모셨는지는 끝난 뒤에 더 자주 묻고, 실수로 지우면 되살릴
  // 길이 없다. 그래서 지우기를 아예 내놓지 않는다(규칙도 막는다). 기한이 지나면 저절로 지워진다.
  const locked = slip.status === 'done';

  const remove = () => {
    if (locked) return;
    if (!confirming) {
      setConfirming(true);
      return;
    }
    tap('heavy');
    void deleteSlip(team.code, slip.id);
    onNotice('쪽지를 지웠습니다.');
    onClose();
  };

  return (
    <div className="sheet-wrap" onClick={onClose}>
      <section
        className={`sheet slip-sheet ${slip.kind}`}
        role="dialog"
        aria-modal="true"
        aria-label={`${KIND_LABEL[slip.kind]} 쪽지`}
        onClick={(event) => event.stopPropagation()}
      >
        <div className="grip" aria-hidden="true" />

        <table className="paper">
          <tbody>
            <tr>
              <th className="paper-kind" colSpan={2}>
                {KIND_LABEL[slip.kind]}
              </th>
              <th>시간</th>
              <td className="big">{slip.time}</td>
            </tr>
            <tr>
              <th>열차번호</th>
              <td className="big">{slip.train || '—'}</td>
              <th>홈</th>
              <td className="big">{slip.platform || '—'}</td>
            </tr>
            <tr>
              <th>구간</th>
              <td colSpan={3}>
                {routeLabel(slip) || '—'}
                {flagLabels(slip).map((flag) => (
                  <span key={flag} className="chip flag inline">
                    {flag}
                  </span>
                ))}
              </td>
            </tr>
            <tr>
              <th>좌석번호</th>
              <td colSpan={3} className="big">
                {seatLabel(slip) || '—'}
              </td>
            </tr>
            <tr>
              <th>내용</th>
              <td colSpan={3}>
                <div className="needs">
                  {NEEDS.map((need) => (
                    <span key={need.id} className={slip.needs.includes(need.id) ? 'need on' : 'need'} title={need.title}>
                      {need.label}
                    </span>
                  ))}
                </div>
              </td>
            </tr>
            {slip.meet && (
              <tr>
                <th>만날 곳</th>
                <td colSpan={3}>{slip.meet}</td>
              </tr>
            )}
            {slip.phone && (
              <tr>
                <th>연락처</th>
                <td colSpan={3}>
                  {tel ? (
                    <a className="tel" href={tel}>
                      {formatPhone(slip.phone)} · 전화
                    </a>
                  ) : (
                    formatPhone(slip.phone)
                  )}
                </td>
              </tr>
            )}
            {slip.memo && (
              <tr>
                <th>메모</th>
                <td colSpan={3} className="memo">
                  {slip.memo}
                </td>
              </tr>
            )}
          </tbody>
        </table>

        {/* 모시는 도중에 하는 일들. 누르는 즉시 저장되고 팀 모두의 화면에 반영된다. */}
        <div className="checks">
          {CHECKS.map((check) => (
            <label key={check.id} className={slip[check.id] ? 'check on' : 'check'}>
              <input
                type="checkbox"
                checked={slip[check.id]}
                onChange={(event) => {
                  tap('medium');
                  void setCheck(team.code, slip.id, check.id, event.target.checked, me);
                }}
              />
              {check.label}
            </label>
          ))}
        </div>

        <p className="byline">
          <span className={`status ${slip.status}`}>{STATUS_LABEL[slip.status]}</span>
          {slip.assignee && ` 담당 ${mine ? '나' : slip.assignee.name}`}
          {` · 적은 사람 ${slip.createdBy.name || '알 수 없음'}`}
          {slip.updatedBy.uid && slip.updatedBy.uid !== slip.createdBy.uid && ` · 마지막 손댄 사람 ${slip.updatedBy.name}`}
        </p>

        <div className="actions">
          {slip.status === 'waiting' && (
            <button className="primary wide" onClick={() => change('doing')}>
              내가 맡기
            </button>
          )}
          {slip.status === 'doing' && (
            <>
              {takenByOther && (
                <p className="notice">{slip.assignee!.name} 님이 맡고 있습니다.</p>
              )}
              <button className="primary wide done" onClick={() => change('done')}>
                완료
              </button>
            </>
          )}
          {slip.status !== 'waiting' && (
            <button className="secondary" onClick={() => change('waiting')}>
              대기로 되돌리기
            </button>
          )}

          <div className="row">
            <button className="secondary" onClick={onEdit}>
              고치기
            </button>
            <button className="secondary" onClick={() => void share()}>
              공유
            </button>
            {!locked && (
              <button className={confirming ? 'danger' : 'secondary'} onClick={remove}>
                {confirming ? '정말 지우기' : '지우기'}
              </button>
            )}
          </div>
          <button className="ghost wide" onClick={onClose}>
            닫기
          </button>
        </div>
      </section>
    </div>
  );
}
