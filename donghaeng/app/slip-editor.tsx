'use client';

import { useState } from 'react';

import { addSlip, updateSlip, type Team } from '../lib/firebase';
import { tap } from '../lib/platform';
import {
  KIND_LABEL,
  NEEDS,
  emptyDraft,
  normalizeTime,
  type Kind,
  type Need,
  type Person,
  type Slip,
  type SlipDraft,
} from '../lib/slip';

/**
 * 쪽지 쓰기.
 *
 * 전화를 받으면서 한 손으로 적는 화면이다. 그래서:
 * - 숫자 칸은 숫자 키패드가 뜬다. 시각은 `1713`만 쳐도 `17:13`이 된다.
 * - 꼭 채워야 하는 칸은 시각 하나다. 나머지는 모르면 비워 두고, 알게 되면 고친다 —
 *   종이 쪽지도 "행( )"을 비워 둔 채 넘어갔다.
 * - 칸 순서는 종이 양식과 같다. 전화로 듣는 순서도 대개 그렇다.
 */
export function SlipEditor({
  kind,
  slip,
  date,
  team,
  me,
  stations,
  onClose,
  onSaved,
}: {
  kind: Kind;
  slip?: Slip;
  date: string;
  team: Team;
  me: Person;
  /** 오늘 다른 쪽지에 적힌 역들. 같은 역을 다시 칠 일이 많다. */
  stations: string[];
  onClose: () => void;
  onSaved: (message: string) => void;
}) {
  const [draft, setDraft] = useState<SlipDraft>(() =>
    slip
      ? {
          kind: slip.kind,
          date: slip.date,
          time: slip.time,
          train: slip.train,
          platform: slip.platform,
          station: slip.station,
          car: slip.car,
          seat: slip.seat,
          needs: slip.needs,
          phone: slip.phone,
          meet: slip.meet,
          memo: slip.memo,
        }
      : emptyDraft(kind, date)
  );
  const [saving, setSaving] = useState(false);

  const set = <K extends keyof SlipDraft>(key: K, value: SlipDraft[K]) =>
    setDraft((current) => ({ ...current, [key]: value }));

  const toggleNeed = (need: Need) => {
    tap();
    set('needs', draft.needs.includes(need) ? draft.needs.filter((item) => item !== need) : [...draft.needs, need]);
  };

  const time = normalizeTime(draft.time);
  const valid = /^\d{2}:\d{2}$/.test(time);

  const save = async () => {
    if (!valid || saving) return;
    setSaving(true);
    tap('medium');
    const clean: SlipDraft = {
      ...draft,
      time,
      train: draft.train.replace(/^#/, '').trim(),
      platform: draft.platform.trim(),
      station: draft.station.trim().replace(/(행|에서)$/, ''),
      car: draft.car.trim(),
      seat: draft.seat.trim().toUpperCase(),
      phone: draft.phone.trim(),
      meet: draft.meet.trim(),
      memo: draft.memo.trim(),
    };
    try {
      if (slip) {
        await updateSlip(team.code, slip.id, clean, me);
        onSaved('쪽지를 고쳤습니다.');
      } else {
        await addSlip(team.code, clean, me);
        onSaved(`${KIND_LABEL[clean.kind]} 쪽지를 올렸습니다.`);
      }
    } catch {
      setSaving(false);
    }
  };

  const departing = draft.kind === 'depart';

  return (
    <div className="sheet-wrap" onClick={onClose}>
      <section
        className={`sheet editor ${draft.kind}`}
        role="dialog"
        aria-modal="true"
        aria-label={slip ? '쪽지 고치기' : '새 쪽지'}
        onClick={(event) => event.stopPropagation()}
      >
        <div className="grip" aria-hidden="true" />

        <div className="segmented kinds">
          {(['depart', 'arrive'] as const).map((value) => (
            <button key={value} className={draft.kind === value ? `on ${value}` : ''} onClick={() => set('kind', value)}>
              {KIND_LABEL[value]}
            </button>
          ))}
        </div>

        <div className="grid">
          <label className="field">
            <span>시간 *</span>
            <input
              className="big"
              inputMode="numeric"
              placeholder="17:13"
              value={draft.time}
              onChange={(event) => set('time', event.target.value)}
              onBlur={() => set('time', normalizeTime(draft.time))}
              aria-invalid={!valid}
            />
          </label>
          <label className="field">
            <span>열차번호</span>
            <input
              className="big"
              inputMode="numeric"
              placeholder="107"
              value={draft.train}
              onChange={(event) => set('train', event.target.value)}
            />
          </label>
          <label className="field">
            <span>홈</span>
            <input
              className="big"
              inputMode="numeric"
              placeholder="6"
              value={draft.platform}
              onChange={(event) => set('platform', event.target.value)}
            />
          </label>
        </div>

        <label className="field">
          <span>{departing ? '구간 — 어디로 가나요 (행)' : '구간 — 어디서 오나요 (온)'}</span>
          <input
            list="stations"
            placeholder={departing ? '예: 부산' : '예: 동대구'}
            value={draft.station}
            onChange={(event) => set('station', event.target.value)}
          />
          <datalist id="stations">
            {stations.map((station) => (
              <option key={station} value={station} />
            ))}
          </datalist>
        </label>

        <div className="grid two">
          <label className="field">
            <span>호차</span>
            <input
              className="big"
              inputMode="numeric"
              placeholder="2"
              value={draft.car}
              onChange={(event) => set('car', event.target.value)}
            />
          </label>
          <label className="field">
            <span>좌석</span>
            <input
              className="big"
              placeholder="1A"
              autoCapitalize="characters"
              value={draft.seat}
              onChange={(event) => set('seat', event.target.value)}
            />
          </label>
        </div>

        <div className="field">
          <span>내용</span>
          <div className="needs pick">
            {NEEDS.map((need) => (
              <button
                key={need.id}
                type="button"
                className={draft.needs.includes(need.id) ? 'need on' : 'need'}
                aria-pressed={draft.needs.includes(need.id)}
                title={need.title}
                onClick={() => toggleNeed(need.id)}
              >
                {need.label}
              </button>
            ))}
          </div>
        </div>

        <label className="field">
          <span>만날 곳</span>
          <input
            placeholder="예: 17시 3층 대합실"
            value={draft.meet}
            onChange={(event) => set('meet', event.target.value)}
          />
        </label>

        <label className="field">
          <span>연락처</span>
          <input
            type="tel"
            inputMode="tel"
            placeholder="010-0000-0000"
            value={draft.phone}
            onChange={(event) => set('phone', event.target.value)}
          />
          <small>쪽지는 이틀 뒤 연락처와 함께 자동으로 지워집니다.</small>
        </label>

        <label className="field">
          <span>메모</span>
          <textarea
            rows={2}
            placeholder="보호자 동행, 짐 많음 등"
            value={draft.memo}
            onChange={(event) => set('memo', event.target.value)}
          />
        </label>

        <div className="actions">
          <button className={`primary wide ${draft.kind}`} disabled={!valid || saving} onClick={() => void save()}>
            {slip ? '고친 내용 저장' : `${KIND_LABEL[draft.kind]} 쪽지 올리기`}
          </button>
          <button className="ghost wide" onClick={onClose}>
            취소
          </button>
        </div>
      </section>
    </div>
  );
}
