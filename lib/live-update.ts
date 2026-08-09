import { isNativeApp, plugin } from './platform';
import type { ReleaseInfo } from './update';

/**
 * 조용한 업데이트 — 앱을 다시 설치하지 않고 화면만 갈아끼운다.
 *
 * 안드로이드는 사이드로딩한 앱이 APK를 소리 없이 설치하도록 두지 않는다. 설치 확인
 * 화면은 시스템이 띄우는 것이고 어떤 라이브러리로도 없앨 수 없다. 그런데 이 앱에서
 * 실제로 자주 바뀌는 것은 APK가 아니라 그 안에 든 웹 자산이다 — 점수판 배치, 기록
 * 화면, 규칙 계산까지 전부 WebView가 읽는 파일이다. 그건 갈아끼울 수 있다.
 *
 * 그래서 업데이트를 둘로 나눈다.
 *
 *   웹 번들 — 릴리스에 붙은 zip을 조용히 받아 두었다가 *다음 실행*에 적용한다.
 *   APK    — 네이티브가 바뀐 릴리스에서만. 미리 받아 두고 설치 화면만 한 번 띄운다.
 *
 * 다음 실행에 적용하는 것이 핵심이다. 받자마자 WebView를 갈아끼우면 치던 게임 도중에
 * 화면이 새로 뜬다. 점수판에서 그것보다 나쁜 일은 별로 없다.
 *
 * 플러그인이 없으면(웹 브라우저, 또는 플러그인 없이 빌드된 앱) 이 파일의 모든 함수가
 * 아무 일도 하지 않고 돌아간다. 업데이트가 안 되는 것과 앱이 깨지는 것은 다른 일이고,
 * 여기서 잘못될 수 있는 모든 경우는 앞엣것이어야 한다.
 */

const updater = () => plugin('CapacitorUpdater');

/** 한 번 받아 둔 버전을 화면을 옮길 때마다 다시 받지 않게 하는 표시. */
let staged: string | null = null;

/**
 * 이 실행이 멀쩡하다고 알린다. 앱이 뜨자마자, 다른 무엇보다 먼저.
 *
 * 이걸 부르지 않으면 플러그인은 새 번들이 앱을 망가뜨렸다고 보고 다음 실행에서 이전
 * 번들로 되돌린다. 그게 안전장치이고, 그래서 실패해도 사용자는 쓰던 앱을 계속 쓴다 —
 * 다만 정상적으로 떴을 때 이 신호를 보내지 않으면 새 번들이 영영 자리를 못 잡는다.
 */
export async function confirmBundle(): Promise<void> {
  try {
    await updater()?.notifyAppReady?.();
  } catch {
    // 플러그인이 없거나 웹이다. 둘 다 정상이다.
  }
}

export type StageResult =
  /** 플러그인이 없다 — 웹이거나 이 기능 없이 빌드된 앱. */
  | { state: 'unsupported' }
  /** 이 릴리스에는 웹 번들이 없다. 네이티브까지 바뀐 릴리스가 그렇다. */
  | { state: 'no-bundle' }
  /** 받아 두었다. 다음에 앱을 켜면 새 화면이다. */
  | { state: 'ready'; version: string }
  | { state: 'error'; reason: string };

/**
 * 새 웹 번들을 받아 다음 실행 자리에 놓는다.
 *
 * 이미 받아 둔 번들이면 다시 받지 않는다. 당구장 지하에서 3G로 켤 때마다 몇 MB를
 * 다시 내려받는 앱이 되어서는 안 된다.
 */
export async function stageWebBundle(release: ReleaseInfo): Promise<StageResult> {
  const api = updater();
  if (!isNativeApp() || !api?.download || !api?.next) return { state: 'unsupported' };
  if (!release.bundle) return { state: 'no-bundle' };
  if (staged === release.version) return { state: 'ready', version: release.version };

  try {
    // 받아 둔 것 중에 같은 버전이 있으면 그걸 쓴다.
    const existing = (await api.list?.())?.bundles ?? [];
    const found = existing.find((entry: any) => entry?.version === release.version);

    const bundle =
      found ?? (await api.download({ url: release.bundle.url, version: release.version }));
    if (!bundle?.id) return { state: 'error', reason: '번들을 받지 못했습니다.' };

    await api.next({ id: bundle.id });
    staged = release.version;
    return { state: 'ready', version: release.version };
  } catch (error: any) {
    return { state: 'error', reason: error?.message ?? '알 수 없는 오류' };
  }
}

/**
 * 지금 깔려 있는 *APK*의 버전.
 *
 * 웹 번들이 바뀌면 `GRAFT_PUBLIC_APP_VERSION`은 새 번들의 값이 된다 — 그건 화면의
 * 버전이지 껍데기의 버전이 아니다. 껍데기가 몇인지는 네이티브만 알고, 그래서 여기서
 * 묻는다. 플러그인이 없으면 둘이 같으므로 빌드에 박힌 값이 곧 답이다.
 */
export async function nativeVersion(): Promise<string | null> {
  try {
    const current = await updater()?.current?.();
    return current?.native ?? null;
  } catch {
    return null;
  }
}
