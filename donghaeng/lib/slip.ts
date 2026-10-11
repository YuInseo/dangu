/**
 * 쪽지 한 장.
 *
 * 종이 쪽지에 적던 칸을 그대로 옮겼다. 칸 이름도 종이에 인쇄된 말을 따른다 — 쓰는
 * 사람이 이미 아는 말을 바꾸면, 앱을 배우는 일이 하나 더 생긴다.
 *
 *   ┌ 도착 ─────────┬ 시간 17:28 ┐
 *   │ 열차번호 46   │ 홈   6     │
 *   │ 구간      동대구 (온)      │
 *   │ 좌석번호  2-1A             │
 *   │ 내용  휠필 (리프트) 시각 무표 │
 *   └───────────────────────────┘
 *
 * 출발 쪽은 인쇄된 양식 없이 손으로 적던 것이라 칸이 조금 더 있다: 연락처(5299-3785)와
 * 만날 곳(17시 3층 대합실). 도착 쪽에도 같은 칸을 두되 비워 둬도 된다.
 */

export type Kind = 'depart' | 'arrive';

export const KIND_LABEL: Record<Kind, string> = {
  depart: '출발',
  arrive: '도착',
};

/**
 * 내용 칸에 동그라미 치던 것들. 순서는 종이 양식과 같다.
 *
 * 줄임말을 풀어 쓰지 않는다. 현장에서 "휠필"은 이미 한 단어이고, 칩에 들어갈 글자는
 * 짧을수록 한 줄에 다 보인다. 풀어 쓴 말은 길게 누르면 뜨는 설명으로만 둔다.
 */
export const NEEDS = [
  { id: 'wheelchair', label: '휠필', title: '휠체어 필요' },
  { id: 'lift', label: '리프트', title: '리프트 필요' },
  { id: 'visual', label: '시각', title: '시각장애' },
  { id: 'noTicket', label: '무표', title: '승차권 없음' },
] as const;

export type Need = (typeof NEEDS)[number]['id'];

export type Status = 'waiting' | 'doing' | 'done';

export const STATUS_LABEL: Record<Status, string> = {
  waiting: '대기',
  doing: '진행',
  done: '완료',
};

export interface Person {
  uid: string;
  name: string;
}

export interface Slip {
  id: string;
  kind: Kind;
  /** `YYYY-MM-DD`, 기기의 지역 날짜. 하루치를 한 번에 묻는 열쇠다. */
  date: string;
  /** `HH:MM`. 열차 시각 — 출발이면 떠나는 시각, 도착이면 닿는 시각. */
  time: string;
  /** 열차번호. 앞에 붙던 `#`은 빼고 숫자만. */
  train: string;
  /** 타는 곳(홈). */
  platform: string;
  /**
   * 구간 — 상대 역 하나.
   *
   * 출발이면 어디로 가는지(종이의 "행( )"), 도착이면 어디서 오는지(종이의 "온").
   * 한 칸으로 둔 이유는 쪽지가 이 역의 것이기 때문이다. 이 역은 언제나 나머지 한 끝이다.
   */
  station: string;
  /** 호차. `2-1A`의 `2`. */
  car: string;
  /** 좌석. `2-1A`의 `1A`. */
  seat: string;
  needs: Need[];
  /** 고객 연락처. 하루가 지나면 쪽지와 함께 지워진다(`RETENTION_DAYS`). */
  phone: string;
  /** 만날 곳. "17시 3층 대합실". */
  meet: string;
  memo: string;
  /** 행신까지 가는(오는) 열차인지. 서울에서 끝나지 않고 행신으로 이어지는 열차는 따로 챙긴다. */
  haengsin: boolean;
  /** 온라인으로 신청한 도움인지. 종이 쪽지에 "온"이라고 동그라미 치던 것. */
  online: boolean;
  /** 승무원에게 넘겼는지. 모시는 도중에 펼친 쪽지에서 바로 체크한다. */
  crewHandover: boolean;
  /** 상대 역(출발이면 도착역, 도착이면 출발역)에 전화했는지. */
  stationCalled: boolean;
  status: Status;
  /** 맡은 사람. 대기 중이면 없다. */
  assignee: Person | null;
  createdBy: Person;
  createdAt: number;
  updatedAt: number;
  updatedBy: Person;
}

/** 새 쪽지를 쓸 때 채우는 칸들. 나머지는 저장하면서 붙는다. */
export type SlipDraft = Pick<
  Slip,
    | 'kind'
  | 'date'
  | 'time'
  | 'train'
  | 'platform'
  | 'station'
  | 'car'
  | 'seat'
  | 'needs'
  | 'phone'
  | 'meet'
  | 'memo'
  | 'haengsin'
  | 'online'
>;

/** 진행하면서 체크하는 것들. 펼친 쪽지에서 누르면 곧바로 팀 모두에게 반영된다. */
export const CHECKS = [
  { id: 'crewHandover', label: '승무원 인계' },
  { id: 'stationCalled', label: '해당역 전화' },
] as const;

export type CheckId = (typeof CHECKS)[number]['id'];

/** 체크한 표시들. 목록·펼친 쪽지·공유 글에 같은 말로 붙는다. */
export function flagLabels(slip: Pick<Slip, 'haengsin' | 'online'>): string[] {
  return [slip.haengsin && '행신행', slip.online && '온라인'].filter(Boolean) as string[];
}

/**
 * 쪽지를 며칠 남겨 두는지.
 *
 * 쪽지에는 고객 연락처가 적혀 있다. 종이 쪽지는 근무가 끝나면 버려졌고, 디지털 쪽지도
 * 그래야 한다 — 남겨 둘 이유가 없는 개인정보는 남기지 않는다. 어제 것까지는 "어제 그
 * 고객 몇 호차였지" 하고 찾을 일이 있어 이틀을 둔다.
 */
export const RETENTION_DAYS = 2;

/* 날짜와 시각 ------------------------------------------------------- */

const pad = (value: number) => String(value).padStart(2, '0');

/** 기기의 지역 날짜. UTC로 자르면 아침 9시 전의 쪽지가 어제로 간다. */
export function dateKey(at: Date = new Date()): string {
  return `${at.getFullYear()}-${pad(at.getMonth() + 1)}-${pad(at.getDate())}`;
}

export function shiftDate(key: string, days: number): string {
  const [year, month, day] = key.split('-').map(Number);
  return dateKey(new Date(year, month - 1, day + days));
}

export function nowTime(at: Date = new Date()): string {
  return `${pad(at.getHours())}:${pad(at.getMinutes())}`;
}

/** 날짜 제목. 오늘·어제·내일은 말로, 나머지는 `10월 9일 (목)`. */
export function dateTitle(key: string, today = dateKey()): string {
  if (key === today) return '오늘';
  if (key === shiftDate(today, -1)) return '어제';
  if (key === shiftDate(today, 1)) return '내일';
  const [year, month, day] = key.split('-').map(Number);
  const weekday = '일월화수목금토'[new Date(year, month - 1, day).getDay()];
  return `${month}월 ${day}일 (${weekday})`;
}

/**
 * 손으로 적던 시각을 `HH:MM`으로.
 *
 * 숫자 키패드로 `1713`만 쳐도 되게 한다. 승강장에서 콜론을 찾아 누르는 건 한 손으로
 * 하기 어렵다. 알아볼 수 없는 값은 그대로 돌려줘서 사람이 고치게 한다.
 */
export function normalizeTime(input: string): string {
  const digits = input.replace(/\D/g, '');
  if (digits.length === 3 || digits.length === 4) {
    const hours = Number(digits.slice(0, digits.length - 2));
    const minutes = Number(digits.slice(-2));
    if (hours < 24 && minutes < 60) return `${pad(hours)}:${pad(minutes)}`;
  }
  return input.trim();
}

/** 열차 시각까지 남은 분. 지났으면 음수. 날짜가 오늘이 아니면 `null`. */
export function minutesUntil(slip: Pick<Slip, 'date' | 'time'>, now: Date = new Date()): number | null {
  if (slip.date !== dateKey(now)) return null;
  const match = /^(\d{1,2}):(\d{2})$/.exec(slip.time);
  if (!match) return null;
  return Number(match[1]) * 60 + Number(match[2]) - (now.getHours() * 60 + now.getMinutes());
}

/* 보여 주기 --------------------------------------------------------- */

/** `2-1A`. 둘 중 하나만 있으면 그것만. */
export function seatLabel(slip: Pick<Slip, 'car' | 'seat'>): string {
  if (slip.car && slip.seat) return `${slip.car}-${slip.seat}`;
  if (slip.car) return `${slip.car}호차`;
  return slip.seat;
}

/** 구간. 출발이면 `동대구행`, 도착이면 `동대구에서`. */
export function routeLabel(slip: Pick<Slip, 'kind' | 'station'>): string {
  if (!slip.station) return '';
  return slip.kind === 'depart' ? `${slip.station}행` : `${slip.station}에서`;
}

export function needLabels(needs: Need[]): string[] {
  return NEEDS.filter((need) => needs.includes(need.id)).map((need) => need.label);
}

/** 전화 걸기용. 네 자리만 적힌 번호(뒷자리)는 그대로 둔다 — 걸 수 없는 번호를 짐작하지 않는다. */
export function telHref(phone: string): string | null {
  const digits = phone.replace(/\D/g, '');
  return digits.length >= 9 ? `tel:${digits}` : null;
}

/** 휴대폰 번호 모양으로. `01052993785` → `010-5299-3785`. 모르는 모양은 그대로. */
export function formatPhone(phone: string): string {
  const digits = phone.replace(/\D/g, '');
  if (/^01\d{8,9}$/.test(digits)) {
    return digits.length === 10
      ? `${digits.slice(0, 3)}-${digits.slice(3, 6)}-${digits.slice(6)}`
      : `${digits.slice(0, 3)}-${digits.slice(3, 7)}-${digits.slice(7)}`;
  }
  return phone.trim();
}

/**
 * 메신저로 보낼 글.
 *
 * 앱을 깔지 않은 사람(다른 역, 차장)에게도 쪽지를 넘길 일이 있다. 그때 받는 쪽이 읽는
 * 것은 이 몇 줄뿐이므로, 종이 쪽지를 사진으로 찍어 보내던 것과 같은 정보가 들어 있어야
 * 한다.
 */
export function shareText(slip: Slip): string {
  const lines = [
    `[${KIND_LABEL[slip.kind]}] ${slip.time}${slip.train ? ` · ${slip.train}열차` : ''}${slip.platform ? ` · ${slip.platform}번 홈` : ''}`,
  ];
  const route = routeLabel(slip);
  const seat = seatLabel(slip);
  if (route || seat) lines.push([route, seat && `좌석 ${seat}`].filter(Boolean).join(' · '));
  const flags = flagLabels(slip);
  if (flags.length) lines.push(flags.join(' · '));
  const done = CHECKS.filter((check) => slip[check.id]).map((check) => `${check.label} ✓`);
  if (done.length) lines.push(done.join(' · '));
  const needs = needLabels(slip.needs);
  if (needs.length) lines.push(`내용: ${needs.join(', ')}`);
  if (slip.meet) lines.push(`만날 곳: ${slip.meet}`);
  if (slip.phone) lines.push(`연락처: ${formatPhone(slip.phone)}`);
  if (slip.memo) lines.push(slip.memo);
  if (slip.assignee) lines.push(`담당: ${slip.assignee.name} (${STATUS_LABEL[slip.status]})`);
  return lines.join('\n');
}

/**
 * 하루치 쪽지의 순서.
 *
 * 끝난 것은 아래로 내린다 — 지금 봐야 하는 건 아직 안 끝난 것이다. 그 안에서는 열차
 * 시각 순서다. 시각이 같으면 먼저 적은 것이 위로.
 */
export function sortSlips(slips: Slip[]): Slip[] {
  return [...slips].sort((a, b) => {
    const doneA = a.status === 'done' ? 1 : 0;
    const doneB = b.status === 'done' ? 1 : 0;
    if (doneA !== doneB) return doneA - doneB;
    if (a.time !== b.time) return a.time < b.time ? -1 : 1;
    return a.createdAt - b.createdAt;
  });
}

/** 빈 쪽지. 시각은 지금으로 채운다 — 대개 곧 올 열차를 적으므로 시만 맞으면 분만 고치면 된다. */
export function emptyDraft(kind: Kind, date = dateKey()): SlipDraft {
  return {
    kind,
    date,
    time: nowTime(),
    train: '',
    platform: '',
    station: '',
    car: '',
    seat: '',
    needs: [],
    phone: '',
    meet: '',
    memo: '',
    haengsin: false,
    online: false,
  };
}

/** Firestore에서 온 값을 믿을 수 있는 모양으로. 빠진 칸이 있어도 화면이 깨지지 않게. */
export function toSlip(id: string, data: any): Slip {
  const someone: Person = { uid: '', name: '' };
  const kind: Kind = data?.kind === 'arrive' ? 'arrive' : 'depart';
  const status: Status = data?.status === 'doing' || data?.status === 'done' ? data.status : 'waiting';
  const text = (value: unknown) => (typeof value === 'string' ? value : '');
  return {
    id,
    kind,
    date: text(data?.date),
    time: text(data?.time),
    train: text(data?.train),
    platform: text(data?.platform),
    station: text(data?.station),
    car: text(data?.car),
    seat: text(data?.seat),
    needs: Array.isArray(data?.needs)
      ? data.needs.filter((need: unknown) => NEEDS.some((known) => known.id === need))
      : [],
    phone: text(data?.phone),
    meet: text(data?.meet),
    memo: text(data?.memo),
    haengsin: data?.haengsin === true,
    online: data?.online === true,
    crewHandover: data?.crewHandover === true,
    stationCalled: data?.stationCalled === true,
    status,
    assignee: data?.assignee?.uid ? { uid: text(data.assignee.uid), name: text(data.assignee.name) } : null,
    createdBy: data?.createdBy?.uid ? data.createdBy : someone,
    createdAt: Number(data?.createdAt) || 0,
    updatedAt: Number(data?.updatedAt) || 0,
    updatedBy: data?.updatedBy?.uid ? data.updatedBy : someone,
  };
}
