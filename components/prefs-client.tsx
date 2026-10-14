'use client';

import type { ReactNode } from 'react';
import { setPref, usePrefs, type Prefs } from '@/lib/prefs';
import { Switch } from './dsp/ui';

/** 룬의 설정 줄: 왼쪽에 이름(과 설명), 오른쪽에 조작. */
export function PrefRow({ label, hint, children }: { label: string; hint?: string; children: ReactNode }) {
  return (
    <div className="pref-row">
      <div>
        <div className="pref-label">{label}</div>
        {hint && <div className="pref-hint">{hint}</div>}
      </div>
      <div className="pref-control">{children}</div>
    </div>
  );
}

function Toggle({ k, label }: { k: keyof Prefs; label: string }) {
  const prefs = usePrefs();
  return <Switch label={label} on={prefs[k] as boolean} onChange={(v) => setPref(k, v as never)} />;
}

function Select<K extends keyof Prefs>({ k, options, label }: { k: K; options: { value: Prefs[K]; label: string }[]; label: string }) {
  const prefs = usePrefs();
  return (
    <select
      className="select"
      aria-label={label}
      value={String(prefs[k])}
      onChange={(e) => setPref(k, options.find((o) => String(o.value) === e.target.value)!.value)}
    >
      {options.map((o) => (
        <option key={String(o.value)} value={String(o.value)}>
          {o.label}
        </option>
      ))}
    </select>
  );
}

export function GeneralPrefs() {
  return (
    <>
      <h3 className="pref-group">탐색</h3>
      <PrefRow label="앨범 표지에 Hi-Res 표시" hint="24비트나 48kHz를 넘는 무손실 앨범의 표지 귀퉁이에 작은 표시를 답니다.">
        <Toggle k="hiresMark" label="Hi-Res 표시" />
      </PrefRow>
      <PrefRow label="곡 목록의 포맷" hint="앨범 화면에서 곡마다 포맷(예: FLAC 24/96)을 언제 보여 줄지.">
        <Select
          k="formatColumn"
          label="곡 목록의 포맷"
          options={[
            { value: 'diff', label: '앨범과 다를 때만' },
            { value: 'always', label: '늘' },
            { value: 'never', label: '보이지 않음' },
          ]}
        />
      </PrefRow>
      <PrefRow label="홈에 오늘의 발견 보이기">
        <Toggle k="homeDiscover" label="오늘의 발견" />
      </PrefRow>
      <h3 className="pref-group">플레이어</h3>
      <PrefRow label="진행 막대를 파형으로" hint="오디오 분석이 끝난 곡은 진행 막대 대신 파형 위에서 탐색합니다.">
        <Toggle k="waveform" label="파형 진행 막대" />
      </PrefRow>
      <PrefRow label="볼륨을 dB로 표시" hint="100%가 0dB입니다. 볼륨을 내리면 신호 경로에 디지털 볼륨 단계가 생깁니다.">
        <Toggle k="volumeDb" label="볼륨 dB" />
      </PrefRow>
    </>
  );
}

export function PlayPrefs() {
  return (
    <>
      <PrefRow
        label="곡을 눌렀을 때"
        hint="앨범·아티스트·검색 화면의 곡 목록에서 곡 하나를 눌렀을 때 할 일. 다른 동작은 곡 옆 ⋯ 메뉴에 늘 있습니다."
      >
        <Select
          k="trackClick"
          label="곡을 눌렀을 때"
          options={[
            { value: 'playFrom', label: '여기서부터 재생' },
            { value: 'playNow', label: '이 곡만 지금 재생' },
            { value: 'next', label: '다음에 재생' },
            { value: 'queue', label: '대기열 끝에 추가' },
          ]}
        />
      </PrefRow>
      <PrefRow label="앨범 카드의 재생 버튼" hint="표지 위에 뜨는 재생 버튼.">
        <Select
          k="cardPlay"
          label="앨범 카드의 재생 버튼"
          options={[
            { value: 'play', label: '처음부터 재생' },
            { value: 'shuffle', label: '셔플 재생' },
          ]}
        />
      </PrefRow>
    </>
  );
}

/** 취향에 따라 감추는 덩어리. 서버에서 그린 것을 브라우저가 거른다. */
export function PrefGate({ pref, children }: { pref: 'homeDiscover'; children: ReactNode }) {
  const prefs = usePrefs();
  return prefs[pref] ? <>{children}</> : null;
}
