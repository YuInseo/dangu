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

export interface ReleaseInfo {
  version: string;
  notes: string;
  url: string;
  /** 안드로이드 APK 자산. 웹에서 열었거나 APK가 없는 릴리스면 없다. */
  apk?: { url: string; name: string; size: number };
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

/** 최신 릴리스를 묻는다. 토큰이 없으므로 공개 저장소에서만 동작한다. */
export async function checkForUpdate(): Promise<UpdateCheck> {
  const repo = repository();
  if (!repo) return { state: 'unconfigured' };

  try {
    const response = await fetch(`https://api.github.com/repos/${repo}/releases/latest`, {
      headers: { accept: 'application/vnd.github+json' },
      // 릴리스 확인이 캐시된 답을 보면 방금 올린 버전을 못 본다.
      cache: 'no-store',
    });
    if (!response.ok) {
      return {
        state: 'error',
        reason:
          response.status === 404
            ? '아직 릴리스가 없습니다.'
            : `업데이트 정보를 가져오지 못했습니다 (HTTP ${response.status}).`,
      };
    }

    const data = await response.json();
    const asset = (data.assets ?? []).find((entry: any) => String(entry.name).endsWith('.apk'));
    const release: ReleaseInfo = {
      version: String(data.tag_name ?? '').replace(/^v/i, ''),
      notes: String(data.body ?? '').trim(),
      url: String(data.html_url ?? ''),
      publishedAt: String(data.published_at ?? ''),
      apk: asset
        ? { url: asset.browser_download_url, name: asset.name, size: Number(asset.size ?? 0) }
        : undefined,
    };

    const current = currentVersion();
    if (!release.version) return { state: 'error', reason: '릴리스에 태그가 없습니다.' };
    return isNewer(release.version, current)
      ? { state: 'available', version: release.version, release }
      : { state: 'current', version: current };
  } catch (error: any) {
    return { state: 'error', reason: error?.message ?? '네트워크 오류' };
  }
}

export type InstallProgress = { downloaded: number; total: number };

export type InstallResult =
  | { ok: true; via: 'installer' | 'browser' }
  | { ok: false; reason: string };

/**
 * 새 버전을 받아서 설치 화면까지 띄운다.
 *
 * 셸에 파일을 저장하고 열 수 있는 플러그인이 있으면 앱 안에서 진행률을 보여주며 받고,
 * 없으면 시스템 브라우저로 넘긴다. 후자도 결과는 같다 — 다운로드 알림을 누르면 같은
 * 설치 화면이 뜬다. 브라우저 경로가 있기 때문에 이 기능은 플러그인 없이도 성립한다.
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

  const filesystem = plugin('Filesystem');
  const opener = plugin('FileOpener') ?? plugin('FileOpenerPlugin');

  if (filesystem?.writeFile && opener?.open) {
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
      const base64 = await blobToBase64(blob);

      const written = await filesystem.writeFile({
        path: release.apk.name,
        data: base64,
        directory: 'CACHE',
        recursive: true,
      });

      await opener.open({
        filePath: written?.uri ?? written?.path,
        contentType: 'application/vnd.android.package-archive',
      });
      return { ok: true, via: 'installer' };
    } catch (error: any) {
      // 받다가 실패했으면 브라우저로 넘긴다. 업데이트를 못 하는 것보다 낫다.
      await openExternal(release.apk.url);
      return { ok: true, via: 'browser' };
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
