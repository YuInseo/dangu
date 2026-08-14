import type { FirebaseApp } from 'firebase/app';
import type { Auth, User } from 'firebase/auth';
import type { Firestore } from 'firebase/firestore';

import { isNativeApp, plugin, preferencesGet, preferencesSet } from './platform';
import { SCHEMA, migrate, type GameSummary } from './game';

/**
 * Firebase — 있으면 쓰고 없으면 없는 대로 돈다.
 *
 * 설정값이 없으면 이 파일의 모든 함수가 `null`이나 빈 결과를 돌려준다. 그게 기본
 * 상태이고, 앱은 그 상태에서도 완전히 동작한다: 게임은 기기에 저장되고, 로그인 화면은
 * "설정되지 않음"이라고 말한다. 클라우드 저장은 로그인한 사람에게 붙는 기능이지
 * 앱이 켜지기 위한 조건이 아니다.
 *
 * SDK는 동적으로만 불러온다 — 첫 화면에 Firebase 300KB가 따라오면 당구장 3G에서
 * 점수판이 늦게 뜬다. 로그인 버튼을 누른 사람만 그 값을 치른다.
 */

export interface FirebaseConfig {
  apiKey: string;
  authDomain: string;
  projectId: string;
  appId: string;
  storageBucket?: string;
  messagingSenderId?: string;
}

const CONFIG_KEY = 'dangu.firebase';

/**
 * 설정값은 두 곳에서 온다: 빌드에 박힌 값과, 사용자가 앱 안에서 직접 넣은 값.
 *
 * 뒤엣것이 이긴다. 사이드로딩으로 배포하는 앱이라 "환경변수를 넣고 다시 빌드하세요"는
 * 대부분의 사용자에게 할 수 없는 요구이고, Firebase 웹 설정은 원래 공개되는 값이라
 * 기기에 평문으로 두어도 잃을 것이 없다 — 보호는 Firestore 규칙이 한다.
 */
let cached: FirebaseConfig | null | undefined;

/** 빌드 시점에 번들에 박히는 값들. */
export function firebaseConfig(): FirebaseConfig | null {
  const config = {
    apiKey: process.env.GRAFT_PUBLIC_FIREBASE_API_KEY ?? '',
    authDomain: process.env.GRAFT_PUBLIC_FIREBASE_AUTH_DOMAIN ?? '',
    projectId: process.env.GRAFT_PUBLIC_FIREBASE_PROJECT_ID ?? '',
    appId: process.env.GRAFT_PUBLIC_FIREBASE_APP_ID ?? '',
    storageBucket: process.env.GRAFT_PUBLIC_FIREBASE_STORAGE_BUCKET ?? '',
    messagingSenderId: process.env.GRAFT_PUBLIC_FIREBASE_MESSAGING_SENDER_ID ?? '',
  };
  // 넷 중 하나라도 비면 설정되지 않은 것으로 본다. 반쯤 설정된 Firebase는
  // 런타임에 알아보기 어려운 오류만 내므로 아예 꺼진 것으로 취급하는 편이 낫다.
  if (!config.apiKey || !config.authDomain || !config.projectId || !config.appId) return null;
  return config;
}

/** 여섯 값 중 필수 넷이 다 있는지. 반쯤 채워진 설정은 없는 것으로 본다. */
function valid(config: Partial<FirebaseConfig> | null): config is FirebaseConfig {
  return Boolean(config?.apiKey && config.authDomain && config.projectId && config.appId);
}

/** 지금 쓰이는 설정. 기기에 저장된 것이 있으면 그것, 없으면 빌드에 박힌 것. */
export async function loadFirebaseConfig(): Promise<FirebaseConfig | null> {
  if (cached !== undefined) return cached;
  const stored = await preferencesGet(CONFIG_KEY);
  if (stored) {
    try {
      const parsed = JSON.parse(stored);
      if (valid(parsed)) return (cached = parsed);
    } catch {
      // 저장된 값이 깨졌으면 빌드 값으로 내려간다.
    }
  }
  return (cached = firebaseConfig());
}

/**
 * 사용자가 넣은 설정을 저장한다. `null`이면 지우고 빌드 값으로 돌아간다.
 *
 * 저장한 뒤에는 이미 만들어 둔 Firebase 인스턴스를 버린다 — 앱을 다시 켜야 적용되는
 * 설정 화면은 설정 화면이 아니라 안내문이다.
 */
export async function saveFirebaseConfig(config: FirebaseConfig | null): Promise<void> {
  await preferencesSet(CONFIG_KEY, config ? JSON.stringify(config) : '');
  cached = undefined;
  appPromise = null;
}

/**
 * 콘솔에서 복사한 것을 그대로 받아 설정으로 만든다.
 *
 * JSON일 수도 있고 `const firebaseConfig = { apiKey: "…" }` 같은 자바스크립트
 * 조각일 수도 있다. 폰에서 여섯 칸을 따로 채우게 하는 것보다, 붙여넣은 덩어리에서
 * 필요한 값을 꺼내는 편이 실수가 적다.
 */
export function parseFirebaseSnippet(text: string): FirebaseConfig | null {
  const pick = (key: string) =>
    // 템플릿 리터럴 안이라 `\\s`로 써야 정규식에 `\s`가 들어간다.
    new RegExp(`["']?${key}["']?\\s*[:=]\\s*["']([^"']+)["']`).exec(text)?.[1] ?? '';

  const config: FirebaseConfig = {
    apiKey: pick('apiKey'),
    authDomain: pick('authDomain'),
    projectId: pick('projectId'),
    appId: pick('appId'),
    storageBucket: pick('storageBucket'),
    messagingSenderId: pick('messagingSenderId'),
  };
  return valid(config) ? config : null;
}

let appPromise: Promise<{ app: FirebaseApp; auth: Auth; db: Firestore } | null> | null = null;

/** 한 번만 초기화한다. 두 번 부르면 같은 약속을 돌려준다. */
export function firebase() {
  if (appPromise) return appPromise;

  appPromise = (async () => {
    const config = await loadFirebaseConfig();
    if (!config || typeof window === 'undefined') return null;

    const [{ initializeApp, getApps, getApp }, { getAuth }, firestore] = await Promise.all([
      import('firebase/app'),
      import('firebase/auth'),
      import('firebase/firestore'),
    ]);

    const app = getApps().length ? getApp() : initializeApp(config);
    const auth = getAuth(app);
    // 오프라인 지속성: 지하 당구장에서 기록 화면을 열어도 마지막으로 받은 게 보인다.
    const db = firestore.initializeFirestore(app, {
      localCache: firestore.persistentLocalCache({ tabManager: firestore.persistentSingleTabManager({}) }),
    });
    return { app, auth, db };
  })();

  return appPromise;
}

/* 로그인 ------------------------------------------------------------ */

export interface Account {
  uid: string;
  name: string;
  email: string | null;
  photo: string | null;
}

const toAccount = (user: User): Account => ({
  uid: user.uid,
  name: user.displayName ?? user.email?.split('@')[0] ?? '플레이어',
  email: user.email,
  photo: user.photoURL,
});

/** 로그인 상태 구독. 해지 함수를 돌려준다. 설정이 없으면 즉시 `null`을 한 번 알린다. */
export function watchAccount(listener: (account: Account | null) => void): () => void {
  let stop: (() => void) | null = null;
  let cancelled = false;

  void (async () => {
    const instance = await firebase();
    if (!instance || cancelled) return listener(null);
    const { onAuthStateChanged } = await import('firebase/auth');
    if (cancelled) return;
    stop = onAuthStateChanged(instance.auth, (user) => listener(user ? toAccount(user) : null));
  })();

  return () => {
    cancelled = true;
    stop?.();
  };
}

export type SignInResult =
  | { ok: true; account: Account }
  | { ok: false; reason: string; cancelled?: boolean };

/**
 * 구글 로그인.
 *
 * 갈래가 하나 있고, 그건 피할 수 없는 갈래다. 구글은 임베디드 WebView에서 오는 OAuth를
 * 거절한다(`disallowed_useragent`) — 안드로이드 앱 안에서 팝업을 띄우면 로그인 화면 대신
 * 그 오류가 뜬다. 그래서 앱에서는 네이티브 로그인 창을 띄우는 플러그인이 idToken을 받아
 * 오고, 그걸 그대로 Firebase 자격증명으로 바꾼다. 웹에서는 팝업이 정상 경로다.
 *
 * 두 경로 모두 끝은 같다: Firebase의 같은 사용자, 같은 uid. 그래서 폰에서 적은 기록이
 * 브라우저에서 그대로 보인다.
 */
export async function signInWithGoogle(): Promise<SignInResult> {
  const instance = await firebase();
  if (!instance) {
    return {
      ok: false,
      reason: 'Firebase가 설정되지 않았습니다. 설정 화면에서 Firebase 설정을 붙여넣으세요.',
    };
  }

  const auth = await import('firebase/auth');

  try {
    if (isNativeApp()) {
      const native = plugin('FirebaseAuthentication');
      if (!native?.signInWithGoogle) {
        /*
         * 앱을 쓰는 사람에게 `npm install`을 시킬 수는 없다.
         *
         * 이 자리에 닿는 사람은 플러그인이 들어가기 전에 빌드된 APK를 쓰고 있는
         * 사람이고, 그가 할 수 있는 일은 둘뿐이다 — 새 APK를 깔거나, 이 화면 바로
         * 아래에 있는 이메일 로그인을 쓰거나. 할 수 없는 일 대신 할 수 있는 일을 적는다.
         */
        return {
          ok: false,
          reason:
            '이 앱 버전에는 구글 로그인이 들어 있지 않습니다. 아래 이메일 로그인을 쓰거나, 설정에서 새 APK를 설치하세요.',
        };
      }

      const result = await native.signInWithGoogle();
      const idToken = result?.credential?.idToken;
      if (!idToken) return { ok: false, reason: '로그인이 취소되었습니다.', cancelled: true };

      const credential = auth.GoogleAuthProvider.credential(idToken, result.credential?.accessToken);
      const signedIn = await auth.signInWithCredential(instance.auth, credential);
      return { ok: true, account: toAccount(signedIn.user) };
    }

    const provider = new auth.GoogleAuthProvider();
    // 계정이 여러 개인 사람에게 매번 고르게 한다. 당구장 폰은 돌려 쓰는 일이 많다.
    provider.setCustomParameters({ prompt: 'select_account' });
    const signedIn = await auth.signInWithPopup(instance.auth, provider);
    return { ok: true, account: toAccount(signedIn.user) };
  } catch (error: any) {
    const code = String(error?.code ?? '');
    if (code.includes('popup-closed') || code.includes('cancelled')) {
      return { ok: false, reason: '로그인이 취소되었습니다.', cancelled: true };
    }
    if (code.includes('unauthorized-domain')) {
      return {
        ok: false,
        reason: '이 도메인이 Firebase 인증에 등록되어 있지 않습니다. 콘솔의 승인된 도메인에 추가하세요.',
      };
    }
    /*
     * 앱 안의 구글 로그인은 APK에 들어 있는 설정을 읽는다. 그 설정이 없는 판으로
     * 설치된 앱에서는 네이티브 쪽이 "기본 앱이 없다"거나 리소스를 못 찾았다고 던지는데,
     * 그 문장을 그대로 보여 주면 읽는 사람이 할 수 있는 일이 없다. 여기서는 할 수 있는
     * 일을 말한다 — 바로 아래에 그 자리에서 되는 로그인이 있다.
     */
    if (isNativeApp() && /FirebaseApp|default_web_client_id|NotFoundException|12500|DEVELOPER_ERROR|10:/i.test(String(error?.message ?? ''))) {
      return {
        ok: false,
        reason:
          '이 앱 버전에는 구글 로그인에 필요한 설정이 없습니다. 아래 이메일 로그인을 쓰거나, 새 APK를 설치하세요.',
      };
    }
    return { ok: false, reason: error?.message ?? '로그인에 실패했습니다.' };
  }
}

/**
 * 이메일과 비밀번호로.
 *
 * 구글 로그인이 앱 안에서 하려면 네이티브 창이 필요하고, 네이티브 창은 APK 안의 설정을
 * 읽는다 — 즉 APK를 새로 깔아야 켜진다. 이 경로는 전부 자바스크립트라서 웹 번들만
 * 바뀌어도 그 자리에서 동작한다. 조용한 업데이트로 갈 수 있는 유일한 로그인이다.
 *
 * 계정을 만드는 것과 들어가는 것을 버튼 하나로 합치지 않았다. Firebase가 이메일
 * 열거 보호를 켜 두면 "없는 계정"과 "틀린 비밀번호"가 같은 오류(`invalid-credential`)로
 * 오기 때문에, 코드가 둘을 구분해서 알아서 가입시키는 것이 불가능하다. 구분할 수 없는
 * 것을 짐작해서 계정을 만드는 것보다 어느 쪽인지 사람이 말하게 하는 편이 낫다.
 */
export async function signInWithEmail(
  email: string,
  password: string,
  create: boolean
): Promise<SignInResult> {
  const instance = await firebase();
  if (!instance) {
    return { ok: false, reason: 'Firebase가 설정되지 않았습니다.' };
  }

  const auth = await import('firebase/auth');
  try {
    const signedIn = create
      ? await auth.createUserWithEmailAndPassword(instance.auth, email.trim(), password)
      : await auth.signInWithEmailAndPassword(instance.auth, email.trim(), password);
    return { ok: true, account: toAccount(signedIn.user) };
  } catch (error: any) {
    return { ok: false, reason: emailReason(String(error?.code ?? ''), error?.message) };
  }
}

/** Firebase의 오류 코드를 사람 말로. 그대로 보여 주면 영어 코드 한 줄이 남는다. */
function emailReason(code: string, fallback?: string): string {
  if (code.includes('invalid-email')) return '이메일 주소 형식이 아닙니다.';
  if (code.includes('missing-password') || code.includes('weak-password')) {
    return '비밀번호는 6자 이상이어야 합니다.';
  }
  if (code.includes('email-already-in-use')) {
    return '이미 있는 계정입니다. 가입 대신 로그인을 누르세요.';
  }
  // 이메일 열거 보호가 켜져 있으면 없는 계정과 틀린 비밀번호가 같은 코드로 온다.
  if (code.includes('invalid-credential') || code.includes('wrong-password') || code.includes('user-not-found')) {
    return '이메일이나 비밀번호가 맞지 않습니다. 처음이라면 가입을 누르세요.';
  }
  if (code.includes('operation-not-allowed')) {
    return 'Firebase 콘솔에서 이메일/비밀번호 로그인을 켜야 합니다.';
  }
  if (code.includes('too-many-requests')) return '잠시 뒤에 다시 시도하세요.';
  if (code.includes('network')) return '네트워크에 연결되어 있지 않습니다.';
  return fallback ?? '로그인에 실패했습니다.';
}

export async function signOut(): Promise<void> {
  const instance = await firebase();
  if (!instance) return;
  if (isNativeApp()) await plugin('FirebaseAuthentication')?.signOut?.();
  const { signOut: firebaseSignOut } = await import('firebase/auth');
  await firebaseSignOut(instance.auth);
}

/* 기록 동기화 -------------------------------------------------------- */

/**
 * 끝난 게임 하나를 클라우드에 올린다.
 *
 * 실패해도 던지지 않는다. 기기에는 이미 저장되어 있고, 여기서 던지면 네트워크가 없는
 * 당구장에서 게임을 끝낼 때마다 오류 화면이 뜬다. 다음 로그인 때 다시 올라간다.
 */
export async function pushGame(uid: string, summary: GameSummary): Promise<boolean> {
  const instance = await firebase();
  if (!instance) return false;
  try {
    const { doc, setDoc } = await import('firebase/firestore');
    await setDoc(doc(instance.db, 'users', uid, 'games', summary.id), {
      ...migrate(summary),
      schema: SCHEMA,
      // 고쳐진 시각. 두 기기가 같은 판을 서로 다르게 고쳤을 때 나중 것이 이긴다.
      updatedAt: summary.updatedAt ?? Date.now(),
      syncedAt: Date.now(),
    });
    return true;
  } catch {
    return false;
  }
}

/** 클라우드에 있는 기록. 로그인한 사람이 기기를 바꿔도 남아 있는 쪽이다. */
export async function fetchGames(uid: string, max = 100): Promise<GameSummary[]> {
  const instance = await firebase();
  if (!instance) return [];
  try {
    const { collection, getDocs, limit, orderBy, query } = await import('firebase/firestore');
    const snapshot = await getDocs(
      query(collection(instance.db, 'users', uid, 'games'), orderBy('startedAt', 'desc'), limit(max))
    );
    return snapshot.docs.map((entry) => migrate(entry.data() as GameSummary));
  } catch {
    return [];
  }
}

/**
 * 클라우드의 기록을 지켜본다 — 바뀌는 대로.
 *
 * 한 번 읽고 마는 것과 다른 점은 "다른 기기에서 방금 친 판"이다. 그건 이 폰이 아무것도
 * 하지 않는 동안 생기는 일이라, 물어보는 방식으로는 언제 물어야 할지를 알 수 없다.
 * Firestore는 붙어 있는 동안 바뀐 것을 밀어 주므로 그쪽을 쓴다.
 *
 * 돌려주는 함수를 부르면 끊는다. 끊지 않으면 화면을 옮겨도 리스너가 남아 배터리와
 * 읽기 할당량을 계속 쓴다.
 */
export async function watchGames(
  uid: string,
  listener: (games: GameSummary[]) => void,
  max = 200
): Promise<() => void> {
  const instance = await firebase();
  if (!instance) return () => {};
  try {
    const { collection, limit, onSnapshot, orderBy, query } = await import('firebase/firestore');
    return onSnapshot(
      query(
        collection(instance.db, 'users', uid, 'games'),
        orderBy('startedAt', 'desc'),
        limit(max)
      ),
      (snapshot) => listener(snapshot.docs.map((entry) => migrate(entry.data() as GameSummary))),
      // 규칙에 막혔거나 네트워크가 끊긴 경우. 조용히 둔다 — 기기에 있는 기록으로 앱은
      // 그대로 돌아가고, 다음에 붙으면 다시 흐른다.
      () => {}
    );
  } catch {
    return () => {};
  }
}

/**
 * 클라우드에 있는 이 사람의 기록을 전부 지운다. 지운 개수를 돌려준다.
 *
 * 배치 하나에 담을 수 있는 쓰기는 500개까지라 나눠서 보낸다. 기기 쪽 상한(50개)보다
 * 훨씬 많이 쌓일 수 있는 곳이라, 한 번에 다 넣으면 조용히 실패하는 크기가 된다.
 */
export async function deleteAllGames(uid: string): Promise<number> {
  const instance = await firebase();
  if (!instance) return 0;
  try {
    const { collection, getDocs, writeBatch } = await import('firebase/firestore');
    const snapshot = await getDocs(collection(instance.db, 'users', uid, 'games'));

    let batch = writeBatch(instance.db);
    let pending = 0;
    let removed = 0;

    for (const entry of snapshot.docs) {
      batch.delete(entry.ref);
      pending++;
      removed++;
      if (pending === 450) {
        await batch.commit();
        batch = writeBatch(instance.db);
        pending = 0;
      }
    }
    if (pending > 0) await batch.commit();
    return removed;
  } catch {
    return 0;
  }
}

export async function deleteGame(uid: string, id: string): Promise<boolean> {
  const instance = await firebase();
  if (!instance) return false;
  try {
    const { deleteDoc, doc } = await import('firebase/firestore');
    await deleteDoc(doc(instance.db, 'users', uid, 'games', id));
    return true;
  } catch {
    return false;
  }
}
