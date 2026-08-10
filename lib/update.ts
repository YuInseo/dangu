import { getPlatform, isNativeApp, openExternal, plugin } from './platform';

/**
 * 인앱 업데이트 — GitHub 릴리스에서 바로.
 *
 * 스토어를 거치지 않고 배포하는 앱이라, 새 버전이 나온 걸 앱이 스스로 알아야 한다.
 * 릴리스 워크플로가 태그마다 APK를 올리고, 여기서는 최신 릴리스의 태그를 지금 버전과
 * 비교한 다음 APK를 내려받아 설치 화면을 띄운다.
 *
 * 설치 자체는 안드로이드가 한다: 앱이 APK를 조용히 덮어쓰는 건 시스템 앱만 할 수 있는
 * 일이고, 그래야 하는 것도 맞다. 사용자가 마지막에 "설치"를 한 번 누른다.
 */

export interface Asset {
  url: string;
  name: string;
  size: number;
}

export interface ReleaseInfo {
  version: string;
  notes: string;
  url: string;
  /** 안드로이드 APK 자산. 웹에서 열었거나 APK가 없는 릴리스면 없다. */
  apk?: Asset;
  /**
   * 웹 자산 묶음. 이게 있으면 APK 없이 화면만 갈아끼울 수 있다.
   *
   * 앱에서 실제로 자주 바뀌는 게 이쪽이라, 대부분의 업데이트는 이 파일 하나로 끝난다.
   */
  bundle?: Asset;
  /**
   * 이 릴리스의 네이티브 껍데기 지문.
   *
   * 자산 이름(`native-<지문>.txt`)에 적혀 있다 — 파일을 따로 받지 않고 목록만 보고
   * 알 수 있게 하려는 것이다. 이 값이 지금 깔린 APK의 것과 다르면, 그때만 APK가
   * 필요하다.
   */
  nativeKey?: string;
  publishedAt: string;
}

export type UpdateCheck =
  | { state: 'current'; version: string }
  | { state: 'available'; version: string; release: ReleaseInfo }
  | { state: 'unconfigured' }
  | { state: 'error'; reason: string };

export const currentVersion = () => process.env.GRAFT_PUBLIC_APP_VERSION ?? '0.0.0-dev';
const repository = () => process.env.GRAFT_PUBLIC_GITHUB_REPO ?? '';

/**
 * `1.2.10`이 `1.2.9`보다 나중이라는 걸 아는 비교.
 *
 * 문자열 비교였다면 `1.2.10 < 1.2.9`가 되어 열 번째 패치부터 업데이트가 영영 뜨지 않는다.
 * 접두 `v`와 `-dev` 같은 꼬리표는 떼고 숫자만 본다.
 */
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
    const left = a[i] ?? 0;
    const right = b[i] ?? 0;
    if (left !== right) return left > right;
  }
  return false;
}

/**
 * 방금 물어본 답.
 *
 * 토큰 없이 부르는 GitHub API는 IP당 시간에 60번이다. 그런데 그 IP는 이동통신에서
 * 내 것이 아니다 — 통신사 NAT 뒤에 있는 모두가 같은 숫자를 나눠 쓴다. 그래서 앱이
 * 켤 때 한 번, 설정을 열 때 한 번, 그 안에서 APK 판단으로 또 한 번 물으면, 아무 잘못
 * 없이도 어느 저녁에 403이 뜬다.
 *
 * 같은 답을 잠깐 재사용해서 부르는 횟수를 줄인다. 새 릴리스가 10분 늦게 보이는 것은
 * 조용한 업데이트를 쓰는 앱에서 아무 문제가 아니고, "다시 확인"은 이 캐시를 건너뛴다.
 */
const CACHE_MS = 10 * 60 * 1000;
let cached: { at: number; result: UpdateCheck } | null = null;

/**
 * 최신 릴리스를 묻는다. 토큰이 없으므로 공개 저장소에서만 동작한다.
 *
 * `force`면 캐시를 건너뛴다 — 사람이 "다시 확인"을 누른 경우다. 그때는 방금 올린
 * 버전을 보러 온 것이므로 10분 전의 답이 쓸모가 없다.
 */
export async function checkForUpdate(force = false): Promise<UpdateCheck> {
  const repo = repository();
  if (!repo) return { state: 'unconfigured' };

  if (!force && cached && Date.now() - cached.at < CACHE_MS) return cached.result;

  try {
    const response = await fetch(`https://api.github.com/repos/${repo}/releases/latest`, {
      headers: { accept: 'application/vnd.github+json' },
      // 릴리스 확인이 캐시된 답을 보면 방금 올린 버전을 못 본다.
      cache: 'no-store',
    });
    if (!response.ok) {
      /*
       * 한도에 걸린 것은 고장이 아니다.
       *
       * 마지막으로 성공한 답이 있으면 그걸 계속 보여 준다 — 조금 전까지 맞던 답이
       * 갑자기 빨간 오류로 바뀔 이유가 없다. 그것도 없을 때만, 무엇이 일어났고 무엇을
       * 하면 되는지 적는다. 사용자가 할 수 있는 일은 기다리는 것뿐이므로 그렇게 쓴다.
       */
      const limited = response.status === 403 || response.status === 429;
      if (limited && cached) return cached.result;
      return {
        state: 'error',
        reason:
          response.status === 404
            ? '아직 릴리스가 없습니다.'
            : limited
              ? 'GitHub 확인 한도에 걸렸습니다. 잠시 뒤에 저절로 다시 확인합니다.'
              : `업데이트 정보를 가져오지 못했습니다 (HTTP ${response.status}).`,
      };
    }

    const release = describe(await response.json());

    const current = currentVersion();
    if (!release.version) return { state: 'error', reason: '릴리스에 태그가 없습니다.' };
    const result: UpdateCheck = isNewer(release.version, current)
      ? { state: 'available', version: release.version, release }
      : { state: 'current', version: current };
    cached = { at: Date.now(), result };
    return result;
  } catch (error: any) {
    if (cached) return cached.result;
    return { state: 'error', reason: error?.message ?? '네트워크 오류' };
  }
}

/** GitHub 릴리스 JSON 하나를 앱이 쓰는 모양으로. */
function describe(data: any): ReleaseInfo {
  const assets: any[] = data.assets ?? [];
  const pick = (test: (name: string) => boolean): Asset | undefined => {
    const found = assets.find((entry) => test(String(entry.name)));
    return found
      ? { url: found.browser_download_url, name: found.name, size: Number(found.size ?? 0) }
      : undefined;
  };

  return {
    version: String(data.tag_name ?? '').replace(/^v/i, ''),
    notes: String(data.body ?? '').trim(),
    url: String(data.html_url ?? ''),
    publishedAt: String(data.published_at ?? ''),
    apk: pick((name) => name.endsWith('.apk')),
    bundle: pick((name) => name.startsWith('www-') && name.endsWith('.zip')),
    nativeKey: /^native-([0-9a-f]+)\.txt$/.exec(
      assets.map((entry) => String(entry.name)).find((name) => name.startsWith('native-')) ?? ''
    )?.[1],
  };
}

/**
 * 이 릴리스가 APK까지 갈아야 하는 것인지.
 *
 * 판단 근거는 지문 두 개다: 새 릴리스의 것과, *지금 깔린 APK가 나온 릴리스*의 것.
 * 뒤엣것을 빌드에 박아 두지 않는 이유는 그 값이 웹 번들에 실려 다니기 때문이다 —
 * 화면만 갈아끼운 뒤에는 빌드에 박힌 지문이 껍데기의 것이 아니라 새 화면의 것이 된다.
 * 그래서 껍데기의 버전을 네이티브에 묻고, 그 버전의 릴리스를 한 번 더 조회한다.
 *
 * 알 수 없으면 `false`다. 확신이 없을 때 설치 화면을 띄우는 쪽보다, 조용히 웹 번들만
 * 최신으로 두는 쪽이 덜 성가시고 덜 위험하다.
 */
export async function needsApk(release: ReleaseInfo): Promise<boolean> {
  const repo = repository();
  if (!repo || !release.apk) return false;
  // 웹 번들이 아예 없는 릴리스라면 갈아끼울 방법이 APK뿐이다.
  if (!release.bundle) return true;
  if (!release.nativeKey) return false;

  const { nativeVersion } = await import('./live-update');
  const native = await nativeVersion();
  if (!native) return false;
  if (native === release.version) return false;

  // 내 껍데기의 지문은 바뀌지 않는다 — 그걸 알아내는 요청도 한 번이면 된다.
  if (mine.version === native) return mine.key !== '' && mine.key !== release.nativeKey;

  try {
    const response = await fetch(`https://api.github.com/repos/${repo}/releases/tags/v${native}`, {
      headers: { accept: 'application/vnd.github+json' },
    });
    if (!response.ok) return false;
    const found = describe(await response.json());
    mine = { version: native, key: found.nativeKey ?? '' };
    return Boolean(mine.key) && mine.key !== release.nativeKey;
  } catch {
    return false;
  }
}

/**
 * 지금 깔려 있는 APK의 네이티브 지문.
 *
 * 한 번 알아내면 앱이 살아 있는 동안 바뀌지 않는다 — 껍데기가 바뀌려면 설치를 거쳐야
 * 하고, 그러면 이 프로세스는 이미 없다. 그래서 기억해 두고 다시 묻지 않는다.
 */
let mine: { version: string; key: string } = { version: '', key: '' };

export type InstallProgress = { downloaded: number; total: number };

export type InstallResult =
  | { ok: true; via: 'installer' | 'browser' }
  | { ok: false; reason: string };

/** 미리 받아 둔 APK. 설치를 누른 순간 다시 받지 않으려고 기억한다. */
let fetched: { version: string; uri: string } | null = null;

/**
 * APK를 기기 캐시에 내려놓는다. 설치는 하지 않는다 — 그건 안드로이드가 한다.
 *
 * 실패하면 `null`이고, 부르는 쪽은 브라우저 경로로 내려간다.
 */
async function toCache(
  release: ReleaseInfo,
  onProgress?: (progress: InstallProgress) => void
): Promise<string | null> {
  const filesystem = plugin('Filesystem');
  if (!release.apk || !isNativeApp() || !filesystem?.writeFile) return null;

  try {
    const response = await fetch(release.apk.url);
    if (!response.ok) throw new Error(`HTTP ${response.status}`);

    const total = Number(response.headers.get('content-length') ?? release.apk.size);
    // `BlobPart[]`로 모으는 이유는 타입 하나 때문이다: 스트림이 주는 `Uint8Array`는
    // 공유 버퍼일 수도 있는 것으로 타입이 잡혀 있어 `Blob`이 그대로 받지 않는다.
    const chunks: BlobPart[] = [];
    let downloaded = 0;

    const reader = response.body?.getReader();
    if (reader) {
      for (;;) {
        const { done, value } = await reader.read();
        if (done) break;
        chunks.push(value.slice().buffer as ArrayBuffer);
        downloaded += value.length;
        onProgress?.({ downloaded, total });
      }
    } else {
      const buffer = await response.arrayBuffer();
      chunks.push(buffer);
      downloaded = buffer.byteLength;
      onProgress?.({ downloaded, total: downloaded });
    }

    const blob = new Blob(chunks, { type: 'application/vnd.android.package-archive' });
    const written = await filesystem.writeFile({
      path: release.apk.name,
      data: await blobToBase64(blob),
      directory: 'CACHE',
      recursive: true,
    });

    return written?.uri ?? written?.path ?? null;
  } catch {
    return null;
  }
}

/**
 * APK를 미리 받아 둔다. 화면에는 아무것도 뜨지 않는다.
 *
 * 네이티브까지 바뀐 릴리스에서만 부른다. 사용자가 설치를 누를 때 4MB를 그제서야 받기
 * 시작하면, 누른 사람은 아무 반응 없는 버튼을 몇십 초 보고 있게 된다. 미리 받아 두면
 * 누르는 즉시 설치 화면이다.
 */
export async function prefetchApk(release: ReleaseInfo): Promise<boolean> {
  if (fetched?.version === release.version) return true;
  const uri = await toCache(release);
  if (uri) fetched = { version: release.version, uri };
  return Boolean(uri);
}

/**
 * 설치 화면까지 띄운다.
 *
 * 미리 받아 둔 파일이 있으면 그대로 열고, 없으면 지금 받는다. 파일을 저장하거나 열
 * 수 없는 셸에서는 시스템 브라우저로 넘긴다 — 다운로드 알림을 누르면 같은 설치 화면이
 * 뜨므로 결과는 같다. 그 경로가 있기 때문에 이 기능은 플러그인 없이도 성립한다.
 */
export async function installUpdate(
  release: ReleaseInfo,
  onProgress?: (progress: InstallProgress) => void
): Promise<InstallResult> {
  if (!release.apk) {
    // 웹에서 눌렀거나 APK 없는 릴리스. 릴리스 페이지를 열어 주는 게 할 수 있는 전부다.
    await openExternal(release.url);
    return { ok: true, via: 'browser' };
  }

  if (!isNativeApp()) {
    await openExternal(release.apk.url);
    return { ok: true, via: 'browser' };
  }

  const opener = plugin('FileOpener') ?? plugin('FileOpenerPlugin');
  const uri = fetched?.version === release.version ? fetched.uri : await toCache(release, onProgress);

  if (uri && opener?.open) {
    try {
      await opener.open({
        filePath: uri,
        contentType: 'application/vnd.android.package-archive',
      });
      return { ok: true, via: 'installer' };
    } catch {
      // 열지 못했으면 브라우저로 넘긴다. 업데이트를 못 하는 것보다 낫다.
    }
  }

  await openExternal(release.apk.url);
  return { ok: true, via: 'browser' };
}

function blobToBase64(blob: Blob): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => {
      const result = String(reader.result);
      // `data:...;base64,` 접두는 플러그인이 원하지 않는다.
      resolve(result.slice(result.indexOf(',') + 1));
    };
    reader.onerror = () => reject(reader.error);
    reader.readAsDataURL(blob);
  });
}

/** 설정 화면에 그대로 보여줄 한 줄. */
export function platformLine(): string {
  const platform = getPlatform();
  const label =
    platform === 'android' ? '안드로이드 앱' : platform === 'ios' ? 'iOS 앱' : platform === 'pwa' ? '설치된 웹앱' : '웹';
  return `${label} · v${currentVersion()}`;
}
