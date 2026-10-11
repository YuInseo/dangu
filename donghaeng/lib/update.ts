import { isNativeApp, preferencesGet, preferencesSet } from './platform';

/**
 * 자동 업데이트 — GitHub 릴리스 `donghaeng-ci`에서.
 *
 * 당구 점수판과 같은 방식이다. 업데이트를 둘로 나눈다:
 *
 *   화면(웹 번들) — 릴리스의 `donghaeng-www.zip`을 조용히 받아 두었다가 다음에 켤 때
 *                   적용한다. 누를 것도, 설치 화면도 없다. 거의 모든 업데이트가 이쪽이다.
 *   껍데기(APK)  — 네이티브가 바뀐 릴리스에서만. 안드로이드는 사이드로딩한 앱이 APK를
 *                   소리 없이 설치하게 두지 않으므로, 받아서 설치 화면을 한 번 띄운다.
 *
 * 받자마자 화면을 갈아끼우지 않는 이유: 쪽지를 적는 도중에 화면이 새로 뜨면 적던 것이
 * 날아간다. 대신 "지금 적용" 버튼을 둔다.
 *
 * 어느 쪽이 필요한지는 네이티브 지문으로 정한다. CI가 껍데기를 정하는 파일들(graft.config.ts,
 * mobile/)의 해시를 릴리스 본문(`native: …`)과 빌드(`GRAFT_PUBLIC_NATIVE_KEY`)에 같이 적는다.
 */

const REPO = process.env.GRAFT_PUBLIC_GITHUB_REPO || 'YuInseo/dangu';
const TAG = 'donghaeng-ci';
const SHELL_KEY = 'donghaeng.shellKey';

export const currentVersion = () => process.env.GRAFT_PUBLIC_APP_VERSION || '0.0.0-dev';
const bakedKey = () => process.env.GRAFT_PUBLIC_NATIVE_KEY || '';

export interface ReleaseInfo {
  version: string;
  nativeKey: string;
  apkUrl: string | null;
  bundleUrl: string | null;
  pageUrl: string;
}

export type UpdatePlan =
  | { kind: 'none' }
  /** 화면을 받아 두었다. 다음에 켜면 적용된다. */
  | { kind: 'staged'; release: ReleaseInfo; bundleId: string }
  /** 껍데기까지 바뀌었다. APK를 설치해야 한다. */
  | { kind: 'apk'; release: ReleaseInfo };

/** `0.1.10`이 `0.1.9`보다 나중이라는 걸 아는 비교. 문자열 비교면 열 번째부터 거꾸로 간다. */
export function isNewer(candidate: string, current: string): boolean {
  const parse = (value: string) =>
    value
      .replace(/^v/i, '')
      .split('-')[0]
      .split('.')
      .map((part) => Number.parseInt(part, 10) || 0);
  const a = parse(candidate);
  const b = parse(current);
  for (let i = 0; i < Math.max(a.length, b.length); i++) {
    if ((a[i] ?? 0) !== (b[i] ?? 0)) return (a[i] ?? 0) > (b[i] ?? 0);
  }
  return false;
}

/**
 * 최신 릴리스를 묻는다.
 *
 * 토큰 없는 GitHub API는 IP당 시간에 60번이고, 이동통신에서는 그 IP를 남들과 나눠 쓴다.
 * 그래서 한 번 받은 답을 10분 동안 다시 쓴다. "업데이트 확인"을 누르면 건너뛴다.
 */
let cached: { at: number; release: ReleaseInfo | null } | null = null;

export async function latestRelease(force = false): Promise<ReleaseInfo | null> {
  if (!force && cached && Date.now() - cached.at < 10 * 60 * 1000) return cached.release;
  try {
    const response = await fetch(`https://api.github.com/repos/${REPO}/releases/tags/${TAG}`, {
      headers: { accept: 'application/vnd.github+json' },
      cache: 'no-store',
    });
    if (!response.ok) return cached?.release ?? null;
    const data = await response.json();
    const assets: any[] = data.assets ?? [];
    const asset = (test: (name: string) => boolean) =>
      assets.find((entry) => test(String(entry.name)))?.browser_download_url ?? null;
    const release: ReleaseInfo = {
      version: /(\d+\.\d+\.\d+)/.exec(String(data.name ?? ''))?.[1] ?? '',
      nativeKey: /native:\s*([0-9a-f]+)/.exec(String(data.body ?? ''))?.[1] ?? '',
      apkUrl: asset((name) => name.endsWith('.apk')),
      bundleUrl: asset((name) => name === 'donghaeng-www.zip'),
      pageUrl: String(data.html_url ?? ''),
    };
    cached = { at: Date.now(), release };
    return release;
  } catch {
    return cached?.release ?? null;
  }
}

/* 조용한 업데이트 (Capgo 업데이터) ------------------------------------ */

const updater = (): any => (globalThis as any).Capacitor?.Plugins?.CapacitorUpdater ?? null;

/** 이 앱에서 화면만 갈아끼울 수 있는지. 웹이거나 플러그인 없는 껍데기면 못 한다. */
export function liveUpdatable(): boolean {
  const api = updater();
  return Boolean(isNativeApp() && api?.download && api?.next);
}

/**
 * 이 실행이 멀쩡하다고 알린다. 앱이 뜨자마자.
 *
 * 부르지 않으면 플러그인은 새 화면이 앱을 망가뜨렸다고 보고 다음 실행에서 이전 것으로
 * 되돌린다. 그게 안전장치다 — 잘못된 번들을 올려도 앱이 못 켜지는 상태로 남지 않는다.
 */
export async function confirmBundle(): Promise<void> {
  try {
    await updater()?.notifyAppReady?.();
  } catch {
    // 웹이거나 플러그인이 없다. 둘 다 정상이다.
  }
}

/**
 * 지금 깔린 *APK*의 네이티브 지문.
 *
 * 빌드에 박힌 값은 화면이 갈아끼워지면 새 화면의 값이 된다 — 껍데기의 값이 아니다.
 * 그래서 APK에 들어 있던 화면(`builtin`)으로 떠 있을 때 그 값을 기기에 적어 두고,
 * 내려받은 화면으로 떠 있을 때는 적어 둔 값을 읽는다. 새 APK를 깔면 업데이터가 builtin으로
 * 되돌아가므로(`resetWhenUpdate`) 그때 다시 적힌다.
 */
async function shellKey(): Promise<string> {
  let builtin = true;
  try {
    const current = await updater()?.current?.();
    builtin = !current?.bundle?.id || current.bundle.id === 'builtin';
  } catch {
    builtin = true;
  }
  if (builtin) {
    if (bakedKey()) await preferencesSet(SHELL_KEY, bakedKey());
    return bakedKey();
  }
  return (await preferencesGet(SHELL_KEY)) ?? '';
}

/**
 * 할 일을 정하고, 화면 업데이트라면 받아 두기까지 한다.
 *
 * 실패는 전부 조용하다 — 업데이트가 안 되는 건 불편이지 고장이 아니고, 쪽지를 보러 앱을
 * 켠 사람에게 오류 줄을 보여 줄 이유가 없다.
 */
export async function prepareUpdate(force = false): Promise<UpdatePlan> {
  if (!isNativeApp()) return { kind: 'none' };
  const release = await latestRelease(force);
  if (!release?.version || !isNewer(release.version, currentVersion())) return { kind: 'none' };

  const mine = await shellKey();
  // 지문을 모르면 화면만 바꾼다. 확신 없이 설치 화면을 띄우는 쪽이 더 성가시다.
  const shellChanged = Boolean(mine && release.nativeKey && mine !== release.nativeKey);

  if (shellChanged || !liveUpdatable() || !release.bundleUrl) {
    return release.apkUrl ? { kind: 'apk', release } : { kind: 'none' };
  }

  try {
    const api = updater();
    const existing: any[] = (await api.list?.())?.bundles ?? [];
    let bundle = existing.find((entry) => entry?.version === release.version);
    // 받다 만 번들은 다음 실행에서 건너뛰어진다. 지우고 처음부터 받는다.
    if (bundle && bundle.status !== 'success' && bundle.status !== 'pending') {
      await api.delete?.({ id: bundle.id }).catch?.(() => {});
      bundle = null;
    }
    bundle ??= await api.download({ url: release.bundleUrl, version: release.version });
    if (!bundle?.id || bundle.status === 'error') return { kind: 'none' };
    await api.next({ id: bundle.id });
    return { kind: 'staged', release, bundleId: bundle.id };
  } catch {
    return { kind: 'none' };
  }
}

/** 받아 둔 화면을 지금 적용한다. 앱이 새로 뜬다. */
export async function applyNow(bundleId: string): Promise<boolean> {
  try {
    await updater()?.set?.({ id: bundleId });
    return true;
  } catch {
    return false;
  }
}

/**
 * APK를 받는다. 시스템 브라우저가 받아 주고, 다운로드 알림을 누르면 설치 화면이 뜬다.
 *
 * 앱 안에서 받아 설치 화면을 바로 여는 길도 있지만 플러그인 둘(파일 저장, 파일 열기)이
 * 더 필요하다. 브라우저 쪽은 아무것도 더 넣지 않고 같은 설치 화면에 닿는다.
 * (껍데기가 바뀌는 드문 경우에만 이 길을 탄다.)
 */
export function openApk(release: ReleaseInfo): void {
  const url = release.apkUrl ?? release.pageUrl;
  if (!url) return;
  // 셸 안에서 `window.open`은 아무 일도 하지 않는다. 바깥 주소로 이동하면 Capacitor가
  // 그걸 가로채 시스템 브라우저에 넘긴다 — 그쪽이 확실한 길이다.
  if (isNativeApp()) window.location.href = url;
  else window.open(url, '_blank', 'noopener');
}
