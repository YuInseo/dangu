import { preferencesGet, preferencesSet } from './platform';
import { shiftDate } from './slip';

/**
 * 근무표 — 주·야·비·휴가 돌아가는 순서.
 *
 * 교대 근무는 날짜마다 따로 적지 않는다. 순서(예: 주야비휴)와 그 순서가 시작한 날 하나만
 * 알면 어느 날이든 계산된다. 대근이나 연차처럼 순서에서 벗어난 날만 따로 적어 둔다.
 *
 * 내 근무는 나만 본다. 팀과 나누지 않으므로 Firestore가 아니라 기기에 둔다.
 */

export type Shift = 'day' | 'night' | 'off' | 'rest';

export const SHIFTS: Shift[] = ['day', 'night', 'off', 'rest'];

export const SHIFT_LABEL: Record<Shift, string> = { day: '주', night: '야', off: '비', rest: '휴' };

export const SHIFT_NAME: Record<Shift, string> = { day: '주간', night: '야간', off: '비번', rest: '휴무' };

const LETTER: Record<string, Shift> = { 주: 'day', 야: 'night', 비: 'off', 휴: 'rest' };

export interface Schedule {
  /** 순서. 기본은 주야비휴. */
  pattern: Shift[];
  /** 순서의 첫 칸이 걸린 날. 없으면 아직 설정하지 않은 것. */
  anchor: string | null;
  /** 순서에서 벗어난 날. */
  overrides: Record<string, Shift>;
}

export const DEFAULT_PATTERN: Shift[] = ['day', 'night', 'off', 'rest'];

const KEY = 'donghaeng.schedule';

/** `주야비휴` → 근무 목록. 모르는 글자는 버린다. */
export function parsePattern(text: string): Shift[] {
  return [...text].map((letter) => LETTER[letter]).filter(Boolean);
}

export function patternText(pattern: Shift[]): string {
  return pattern.map((shift) => SHIFT_LABEL[shift]).join('');
}

function daysBetween(from: string, to: string): number {
  const [y1, m1, d1] = from.split('-').map(Number);
  const [y2, m2, d2] = to.split('-').map(Number);
  // UTC로 센다. 지역 시각이면 서머타임이 있는 곳에서 하루가 23시간이 된다.
  return Math.round((Date.UTC(y2, m2 - 1, d2) - Date.UTC(y1, m1 - 1, d1)) / 86_400_000);
}

/** 그날의 근무. 설정 전이면 `null`. */
export function shiftOn(schedule: Schedule, date: string): Shift | null {
  if (schedule.overrides[date]) return schedule.overrides[date];
  if (!schedule.anchor || schedule.pattern.length === 0) return null;
  const length = schedule.pattern.length;
  const index = ((daysBetween(schedule.anchor, date) % length) + length) % length;
  return schedule.pattern[index];
}

/**
 * "오늘은 야간"처럼 오늘 근무를 고르면 순서의 시작일을 거꾸로 맞춘다.
 *
 * 시작일을 달력에서 찾아 고르게 하면 대개 틀린다. 오늘 무슨 근무인지는 누구나 안다.
 * 순서에 같은 글자가 둘 있으면(주주야야…) 몇 번째인지 `position`으로 받는다.
 */
export function anchorFor(today: string, position: number): string {
  return shiftDate(today, -position);
}

export async function loadSchedule(): Promise<Schedule> {
  const text = await preferencesGet(KEY);
  try {
    const value = text ? JSON.parse(text) : null;
    if (value && Array.isArray(value.pattern)) {
      return {
        pattern: value.pattern.filter((shift: string) => SHIFTS.includes(shift as Shift)),
        anchor: typeof value.anchor === 'string' ? value.anchor : null,
        overrides: value.overrides && typeof value.overrides === 'object' ? value.overrides : {},
      };
    }
  } catch {
    // 망가진 값이면 처음부터.
  }
  return { pattern: DEFAULT_PATTERN, anchor: null, overrides: {} };
}

export async function saveSchedule(schedule: Schedule): Promise<void> {
  await preferencesSet(KEY, JSON.stringify(schedule));
}
