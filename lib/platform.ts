import { clipboard, haptics, isNativeApp, getPlatform } from 'graft/native';

/**
 * 앱이 기기에 대해 묻는 것 전부. 화면에서는 여기만 부른다.
 *
 * `graft/mobile`을 쓰지 않는 이유는 하나다. 그건 `'use mobile'` 모듈에서만 import할 수
 * 있고, 그 지시자는 "웹에는 정직한 답이 없는 기능"에 붙이는 것이다. 점수판이 쓰는 건
 * 저장소·클립보드·진동뿐이라 셋 다 브라우저에도 답이 있다 — 그래서 웹에서도 같은 파일이
 * 그대로 돈다.
 */

const isBrowser = () => typeof window !== 'undefined';

/** Capacitor 셸이 노출한 플러그인. 웹에서는 항상 null이고, 그럼 아래 대체 경로가 쓰인다. */
function plugin(name: string): any {
  if (!isBrowser()) return null;
  const scope = globalThis as any;
  return scope.Capacitor?.Plugins?.[name] ?? scope.__GRAFT_NATIVE__?.[name] ?? null;
}

export { clipboard, isNativeApp, getPlatform, plugin };

/* 저장소 ------------------------------------------------------------ */

/**
 * 셸에서는 네이티브 설정 저장소, 웹에서는 `localStorage`.
 *
 * 둘의 차이가 하나 있다: 안드로이드에서 `localStorage`는 WebView가 데이터를 비우면 같이
 * 날아가지만 네이티브 저장소는 남는다. 진행 중이던 게임이 앱을 껐다 켜면 사라지는 건
 * 점수판에서 제일 화나는 버그라, 있을 때는 반드시 네이티브 쪽을 쓴다.
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
    // 시크릿 창처럼 저장이 막힌 환경. 게임은 계속 돌아가야 하므로 삼킨다.
  }
}

/* 진동 -------------------------------------------------------------- */

/**
 * 점수 버튼의 촉각 피드백.
 *
 * 큐를 들고 화면을 안 보면서 누르는 버튼이라, 눌렸는지를 손으로 알아야 한다. 웹에서는
 * `navigator.vibrate`, 셸에서는 네이티브 햅틱 — `graft/native`가 이미 그 둘을 하나로
 * 만들어 두었다.
 */
export function tap(style: 'light' | 'medium' | 'heavy' = 'light') {
  void haptics(style);
}

/* 화면 꺼짐 방지 ---------------------------------------------------- */

/**
 * 게임 중에 화면이 꺼지지 않게.
 *
 * 웹에는 Screen Wake Lock이 있고, 셸에는 KeepAwake 플러그인이 있다. 둘 다 없으면 아무
 * 일도 일어나지 않는다 — 화면이 꺼지는 건 불편이지 고장이 아니라서, 없다고 알릴 것도 없다.
 */
export async function keepAwake(on: boolean): Promise<() => void> {
  if (!isBrowser()) return () => {};

  const native = plugin('KeepAwake');
  if (native?.keepAwake) {
    if (on) await native.keepAwake();
    else await native.allowSleep?.();
    return () => void native.allowSleep?.();
  }

  const wakeLock = (navigator as any).wakeLock;
  if (!on || !wakeLock?.request) return () => {};
  try {
    const sentinel = await wakeLock.request('screen');
    return () => void sentinel.release?.();
  } catch {
    return () => {};
  }
}

/* 셸 초기화 --------------------------------------------------------- */

/**
 * 네이티브 셸에서 한 번 해 두어야 하는 것들.
 *
 * 안드로이드는 targetSdk 35부터 앱을 edge-to-edge로 그린다. 그러면 WebView가 상태
 * 표시줄 자리까지 차지해서 시계 위에 앱 제목이 겹치고, WebView는 그 높이를
 * `env(safe-area-inset-top)`으로 알려 주지 않으므로 CSS로는 피할 수 없다. 겹침을
 * 끄는 것이 유일한 답이고, 그건 플러그인만 할 수 있다.
 *
 * 웹에서는 플러그인이 없으므로 아무 일도 일어나지 않는다 — 브라우저에는 애초에
 * 겹칠 상태 표시줄이 없다.
 */
export async function prepareShell(): Promise<void> {
  const statusBar = plugin('StatusBar');
  if (!statusBar) return;
  try {
    await statusBar.setOverlaysWebView?.({ overlay: false });
    // 상태 표시줄을 앱 배경과 같은 색으로: 검은 앱 위의 흰 띠는 잘린 것처럼 보인다.
    await statusBar.setBackgroundColor?.({ color: '#0f1115' });
    await statusBar.setStyle?.({ style: 'DARK' });
  } catch {
    // 플러그인이 있어도 기기가 거절할 수 있다. 겹쳐 보이는 건 불편이지 고장이 아니다.
  }
}

/* 바깥 링크 --------------------------------------------------------- */

/**
 * 앱 밖으로 나가는 링크. 셸 안에서 `window.open`은 WebView를 그 URL로 덮어버리는 수가
 * 있어서, 시스템 브라우저로 여는 플러그인이 있으면 그쪽을 쓴다.
 */
export async function openExternal(url: string): Promise<boolean> {
  if (!isBrowser()) return false;
  const browser = plugin('Browser');
  if (browser?.open) {
    await browser.open({ url });
    return true;
  }
  window.open(url, '_blank', 'noopener,noreferrer');
  return true;
}
