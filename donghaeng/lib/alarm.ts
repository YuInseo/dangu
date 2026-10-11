import { preferencesGet, preferencesSet } from './platform';
import { KIND_LABEL, type Slip } from './slip';

/**
 * 쪽지 알림 — 열차 시각 몇 분 전에 폰이 울린다.
 *
 * 알림은 폰의 알람(AlarmManager)에 걸어 둔다. 그래서 앱을 닫아도, 화면이 꺼져 있어도
 * 울린다. 대신 알람은 걸어 둔 순간의 쪽지를 기억하므로, 앱이 켜져 있는 동안 쪽지가
 * 바뀔 때마다(맡음·완료·시각 고침) 전부 다시 건다. 앱이 꺼진 사이에 남이 완료한 쪽지는
 * 그대로 울린다 — 울리고 나서 열어 보면 완료로 보인다.
 *
 * 웹에는 앱을 닫은 뒤에도 울릴 방법이 없으므로 아무것도 하지 않는다.
 */

export interface AlarmSettings {
  enabled: boolean;
  /** 열차 시각 몇 분 전. 0이면 정각. */
  lead: number;
  /** 내가 맡은 쪽지만 울린다. 끄면 끝나지 않은 팀 쪽지 전부. */
  mineOnly: boolean;
}

export const LEADS = [0, 5, 10, 15, 30];

const KEY = 'donghaeng.alarm';
const DEFAULTS: AlarmSettings = { enabled: true, lead: 10, mineOnly: false };

function plugin(): any {
  return (globalThis as any).Capacitor?.Plugins?.LocalNotifications ?? null;
}

export function alarmsSupported(): boolean {
  return Boolean(plugin());
}

export async function loadAlarmSettings(): Promise<AlarmSettings> {
  try {
    const text = await preferencesGet(KEY);
    return text ? { ...DEFAULTS, ...JSON.parse(text) } : DEFAULTS;
  } catch {
    return DEFAULTS;
  }
}

export async function saveAlarmSettings(settings: AlarmSettings): Promise<void> {
  await preferencesSet(KEY, JSON.stringify(settings));
}

/** 알림 권한. 안드로이드 13부터는 묻지 않으면 알림이 조용히 버려진다. */
export async function ensurePermission(): Promise<boolean> {
  const native = plugin();
  if (!native) return false;
  try {
    const current = await native.checkPermissions();
    if (current?.display === 'granted') return true;
    const asked = await native.requestPermissions();
    return asked?.display === 'granted';
  } catch {
    return false;
  }
}

/** 쪽지 하나에 알림 번호 하나. 플러그인은 32비트 정수만 받는다. */
function idFor(slip: Slip): number {
  let hash = 0;
  for (const char of slip.id) hash = (hash * 31 + char.charCodeAt(0)) | 0;
  return Math.abs(hash) || 1;
}

function slipTime(slip: Slip): Date {
  const [year, month, day] = slip.date.split('-').map(Number);
  const [hour, minute] = slip.time.split(':').map(Number);
  return new Date(year, month - 1, day, hour, minute);
}

/** 알림 본문. 승강장에서 폰을 꺼내 한 줄로 읽히게 — 어디로, 몇 호차, 어디서 만나나. */
function bodyOf(slip: Slip): string {
  return [
    slip.train && `${slip.train}열차`,
    slip.platform && `${slip.platform}번 홈`,
    slip.station && (slip.kind === 'depart' ? `${slip.station}행` : `${slip.station}에서`),
    (slip.car || slip.seat) && `${slip.car ? `${slip.car}호차 ` : ''}${slip.seat}`.trim(),
    slip.meet && `만날 곳 ${slip.meet}`,
  ]
    .filter(Boolean)
    .join(' · ');
}

let lastKey = '';

/**
 * 오늘 쪽지로 알림을 다시 건다. 바뀐 게 없으면 아무것도 하지 않는다.
 *
 * 걸려 있던 동행 알림을 전부 지우고 새로 건다. 하나씩 맞춰 보는 것보다 단순하고, 쪽지는
 * 하루에 수십 장이라 다시 거는 비용이 없다.
 */
export async function syncAlarms(slips: Slip[], myUid: string | null, settings: AlarmSettings): Promise<void> {
  const native = plugin();
  if (!native) return;

  const now = Date.now();
  const wanted = settings.enabled
    ? slips
        .filter((slip) => slip.status !== 'done')
        .filter((slip) => !settings.mineOnly || slip.assignee?.uid === myUid)
        .map((slip) => ({ slip, at: new Date(slipTime(slip).getTime() - settings.lead * 60_000) }))
        .filter(({ at }) => at.getTime() > now)
    : [];

  const key = JSON.stringify(wanted.map(({ slip, at }) => [slip.id, at.getTime(), bodyOf(slip), slip.kind]));
  if (key === lastKey) return;
  lastKey = key;

  try {
    const pending = await native.getPending();
    const ours = (pending?.notifications ?? []).filter((item: any) => item?.extra?.app === 'donghaeng');
    if (ours.length) await native.cancel({ notifications: ours.map((item: any) => ({ id: item.id })) });
    if (!wanted.length) return;

    await native.schedule({
      notifications: wanted.map(({ slip, at }) => ({
        id: idFor(slip),
        title: `${KIND_LABEL[slip.kind]} ${slip.time}${settings.lead ? ` · ${settings.lead}분 전` : ' · 지금'}`,
        body: bodyOf(slip) || '쪽지를 확인하세요.',
        // 잠자기 모드에서도 제시간에. 정확한 알람 권한은 manifest.xml 주입에 있다.
        schedule: { at, allowWhileIdle: true },
        extra: { app: 'donghaeng', slip: slip.id },
      })),
    });
  } catch {
    // 다음에 쪽지가 바뀌면 다시 건다.
    lastKey = '';
  }
}
