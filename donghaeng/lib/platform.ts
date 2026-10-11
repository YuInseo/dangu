import { haptics, isNativeApp, onHardwareBack, share as nativeShare, writeClipboard } from 'graft/native';

/**
 * 앱이 기기에 대해 묻는 것 전부. 화면에서는 여기만 부른다.
 *
 * 전부 웹에도 답이 있는 것들이라(저장소·진동·공유) 같은 파일이 브라우저와 앱에서 그대로
 * 돈다. 없으면 조용히 아무 일도 하지 않는다 — 진동이 안 되는 건 불편이지 고장이 아니다.
 */

const isBrowser = () => typeof window !== 'undefined';

function plugin(name: string): any {
  if (!isBrowser()) return null;
  const scope = globalThis as any;
  return scope.Capacitor?.Plugins?.[name] ?? scope.__GRAFT_NATIVE__?.[name] ?? null;
}

export { isNativeApp };

/**
 * 안드로이드 뒤로 가기를 가로챈다. 시트가 열려 있을 때만 부른다 — 듣는 쪽이 하나라도
 * 있으면 앱이 닫히는 기본 동작이 꺼지기 때문이다. 웹에서는 아무 일도 없다.
 */
export const onBack = onHardwareBack;

/* 저장소 ------------------------------------------------------------ */

/**
 * 셸에서는 네이티브 설정 저장소, 웹에서는 `localStorage`.
 *
 * 이름과 팀 코드만 여기 둔다. 쪽지는 Firestore가 자기 캐시에 갖고 있다.
 */
export async function preferencesGet(key: string): Promise<string | null> {
  if (!isBrowser()) return null;
  const native = plugin('Preferences') ?? plugin('Storage');
  if (native?.get) {
    const result = await native.get({ key });
    return result?.value ?? null;
  }
  try {
    return localStorage.getItem(key);
  } catch {
    return null;
  }
}

export async function preferencesSet(key: string, value: string): Promise<void> {
  if (!isBrowser()) return;
  const native = plugin('Preferences') ?? plugin('Storage');
  if (native?.set) {
    await native.set({ key, value });
    return;
  }
  try {
    localStorage.setItem(key, value);
  } catch {
    // 시크릿 창처럼 저장이 막힌 환경. 다음에 켤 때 다시 물을 뿐이다.
  }
}

/* 진동 -------------------------------------------------------------- */

export function tap(style: 'light' | 'medium' | 'heavy' = 'light') {
  void haptics(style);
}

/* 공유 -------------------------------------------------------------- */

/**
 * 쪽지를 메신저로. 공유 시트가 있으면 그것, 없으면 클립보드.
 *
 * 무엇이 일어났는지를 돌려준다 — 클립보드로 떨어졌으면 "복사했습니다"라고 말해야
 * 사람이 붙여넣으러 간다. 아무 말도 없으면 버튼이 고장 난 줄 안다.
 */
export async function shareText(
  title: string,
  text: string
): Promise<'shared' | 'copied' | 'cancelled' | 'failed'> {
  try {
    const result = await nativeShare({ title, text });
    if (result.supported) {
      if (result.cancelled) return 'cancelled';
      return result.via === 'clipboard' ? 'copied' : 'shared';
    }
  } catch {
    // 시트가 떴다가 실패한 경우. 클립보드로 내려간다.
  }
  try {
    const copied = await writeClipboard(text);
    return copied.supported ? 'copied' : 'failed';
  } catch {
    return 'failed';
  }
}

/* 셸 초기화 --------------------------------------------------------- */

/** 상태 표시줄을 앱의 윗줄과 같은 색으로. 웹에서는 할 일이 없다. */
export async function prepareShell(): Promise<void> {
  const statusBar = plugin('StatusBar');
  if (!statusBar) return;
  try {
    await statusBar.setOverlaysWebView?.({ overlay: false });
    await statusBar.setBackgroundColor?.({ color: '#1d3a6e' });
    await statusBar.setStyle?.({ style: 'DARK' });
  } catch {
    // 기기가 거절할 수 있다. 색이 안 맞는 건 불편이지 고장이 아니다.
  }
}
