import type { Auth } from 'firebase/auth';
import type { Firestore } from 'firebase/firestore';

import { RETENTION_DAYS, dateKey, shiftDate, toSlip, type Person, type Slip, type SlipDraft, type Status } from './slip';

/**
 * Firestore에 두는 것 전부.
 *
 *   teams/{code}                  팀 이름. 코드가 곧 문서 이름이다.
 *   teams/{code}/members/{uid}    들어온 사람. 이 문서가 있어야 쪽지를 읽고 쓴다.
 *   teams/{code}/slips/{id}       쪽지.
 *
 * 팀 코드를 아는 것이 들어갈 자격이다. 코드는 사람이 받아 적을 수 있는 여덟 글자이고
 * 헷갈리는 글자(0과 O, 1과 I)를 뺐다. 팀 목록은 누구도 읽을 수 없으므로(`firestore.rules`)
 * 코드를 모르면 팀이 있는지조차 알 수 없다.
 *
 * 로그인은 구글 계정이다. 한 번 로그인하면 기기에 남으므로 매번 묻지 않고, 폰을 바꿔도
 * 같은 계정이면 같은 사람으로 남는다. 쪽지에 붙는 이름은 구글 이름을 기본으로 하되 고칠 수 있다.
 */

let instance: Promise<{ auth: Auth; db: Firestore } | null> | null = null;

function config() {
  const value = {
    apiKey: process.env.GRAFT_PUBLIC_FIREBASE_API_KEY ?? '',
    authDomain: process.env.GRAFT_PUBLIC_FIREBASE_AUTH_DOMAIN ?? '',
    projectId: process.env.GRAFT_PUBLIC_FIREBASE_PROJECT_ID ?? '',
    appId: process.env.GRAFT_PUBLIC_FIREBASE_APP_ID ?? '',
    storageBucket: process.env.GRAFT_PUBLIC_FIREBASE_STORAGE_BUCKET ?? '',
    messagingSenderId: process.env.GRAFT_PUBLIC_FIREBASE_MESSAGING_SENDER_ID ?? '',
  };
  if (!value.apiKey || !value.authDomain || !value.projectId || !value.appId) return null;
  return value;
}

/** 한 번만 초기화한다. SDK는 처음 부를 때 불러온다. */
export function firebase() {
  if (instance) return instance;
  instance = (async () => {
    const settings = config();
    if (!settings || typeof window === 'undefined') return null;
    const [{ initializeApp, getApps, getApp }, { getAuth }, firestore] = await Promise.all([
      import('firebase/app'),
      import('firebase/auth'),
      import('firebase/firestore'),
    ]);
    const app = getApps().length ? getApp() : initializeApp(settings);
    const auth = getAuth(app);
    // 오프라인 지속성: 신호가 끊긴 승강장에서도 마지막으로 받은 쪽지가 보이고, 거기서
    // 쓴 쪽지는 기기에 먼저 남았다가 붙는 대로 올라간다.
    const db = firestore.initializeFirestore(app, {
      localCache: firestore.persistentLocalCache({ tabManager: firestore.persistentSingleTabManager({}) }),
    });
    return { auth, db };
  })();
  return instance;
}

/** Firebase 오류를 사람 말로. */
export function reason(error: any): string {
  const code = String(error?.code ?? '');
  const message = String(error?.message ?? '');
  if (code.includes('operation-not-allowed')) {
    return 'Firebase 콘솔에서 구글 로그인을 켜야 합니다 (Authentication → 로그인 방법 → Google).';
  }
  if (code.includes('unauthorized-domain')) {
    return '이 주소가 Firebase 인증의 승인된 도메인에 없습니다. 콘솔에서 추가하세요.';
  }
  // 앱 안의 구글 창은 패키지 이름과 서명 지문이 Firebase에 등록돼 있어야 열린다.
  if (/DEVELOPER_ERROR|10:|12500|No credentials available|default_web_client_id/i.test(message)) {
    // 원문을 붙여 둔다. 같은 안내 뒤에 원인이 여럿 숨어 있어서, 원문 없이는 무엇을 고칠지 모른다.
    return `구글 로그인 창을 열지 못했습니다. Firebase 콘솔에 이 앱의 패키지 이름과 서명 키의 SHA-1 지문이 등록돼 있어야 합니다. (원인: ${message})`;
  }
  if (code.includes('permission-denied')) return '권한이 없습니다. 팀 코드를 다시 확인하세요.';
  if (code.includes('unavailable') || code.includes('network')) return '네트워크에 연결되어 있지 않습니다.';
  return message || '알 수 없는 오류가 났습니다.';
}

/* 로그인 ------------------------------------------------------------ */

export interface Account {
  uid: string;
  name: string;
  email: string | null;
}

/** Capacitor 셸의 플러그인. 웹에서는 `null`. */
function nativePlugin(name: string): any {
  if (typeof window === 'undefined') return null;
  return (globalThis as any).Capacitor?.Plugins?.[name] ?? null;
}

function isNative(): boolean {
  return Boolean((globalThis as any).Capacitor?.isNativePlatform?.());
}

/**
 * 로그인 상태를 지켜본다. 해지 함수를 돌려준다.
 *
 * 한 번 로그인하면 Firebase가 기기에 남겨 두므로, 다음에 켤 때는 이 리스너가 곧바로
 * 그 계정을 알려 준다 — 매번 구글 창을 거치지 않는다.
 */
export function watchAccount(listener: (account: Account | null) => void): () => void {
  let stop: (() => void) | null = null;
  let cancelled = false;
  void (async () => {
    const ready = await firebase();
    if (!ready || cancelled) return listener(null);
    const { onAuthStateChanged } = await import('firebase/auth');
    if (cancelled) return;
    stop = onAuthStateChanged(ready.auth, (user) =>
      listener(
        user
          ? { uid: user.uid, name: user.displayName ?? user.email?.split('@')[0] ?? '', email: user.email }
          : null
      )
    );
  })();
  return () => {
    cancelled = true;
    stop?.();
  };
}

/**
 * 구글로 로그인한다. 취소하면 `null`.
 *
 * 앱과 웹의 길이 다르다. 구글은 WebView 안의 OAuth를 거절하므로(`disallowed_useragent`)
 * 앱에서는 네이티브 플러그인이 구글 창을 띄워 idToken을 받아 오고, 그걸 자바스크립트 SDK의
 * 자격증명으로 바꾼다. 웹에서는 팝업이다. 끝은 같다 — 같은 구글 계정이면 같은 uid.
 */
export async function signInWithGoogle(): Promise<Account | null> {
  const ready = await firebase();
  if (!ready) throw new Error('Firebase가 설정되지 않았습니다.');
  const auth = await import('firebase/auth');

  if (isNative()) {
    const native = nativePlugin('FirebaseAuthentication');
    if (!native?.signInWithGoogle) {
      throw new Error('이 앱 버전에는 구글 로그인이 들어 있지 않습니다. 새 APK를 설치하세요.');
    }
    const result = await native.signInWithGoogle();
    const idToken = result?.credential?.idToken;
    if (!idToken) return null;
    const credential = auth.GoogleAuthProvider.credential(idToken, result.credential?.accessToken);
    const { user } = await auth.signInWithCredential(ready.auth, credential);
    return { uid: user.uid, name: user.displayName ?? '', email: user.email };
  }

  const provider = new auth.GoogleAuthProvider();
  // 역 사무실 폰은 여럿이 돌려 쓴다. 매번 계정을 고르게 한다.
  provider.setCustomParameters({ prompt: 'select_account' });
  try {
    const { user } = await auth.signInWithPopup(ready.auth, provider);
    return { uid: user.uid, name: user.displayName ?? '', email: user.email };
  } catch (error: any) {
    if (String(error?.code ?? '').match(/popup-closed|cancelled/)) return null;
    throw error;
  }
}

export async function signOut(): Promise<void> {
  const ready = await firebase();
  if (!ready) return;
  if (isNative()) await nativePlugin('FirebaseAuthentication')?.signOut?.().catch?.(() => {});
  const { signOut: firebaseSignOut } = await import('firebase/auth');
  await firebaseSignOut(ready.auth);
}

/* 팀 ---------------------------------------------------------------- */

// 0·O, 1·I·L을 뺀 서른 글자. 여덟 자리면 6천억 가지가 넘는다.
const ALPHABET = 'ABCDEFGHJKMNPQRSTUVWXYZ2345679';

function newCode(): string {
  const bytes = new Uint8Array(8);
  crypto.getRandomValues(bytes);
  return Array.from(bytes, (byte) => ALPHABET[byte % ALPHABET.length]).join('');
}

/** 사람이 친 코드를 정규화한다. 소문자·공백·하이픈을 받아 준다. */
export function cleanCode(input: string): string {
  return input.toUpperCase().replace(/[^A-Z0-9]/g, '');
}

export interface Team {
  code: string;
  name: string;
}

/** 새 팀을 만들고 만든 사람을 첫 구성원으로 넣는다. */
export async function createTeam(name: string, me: Person): Promise<Team> {
  const ready = await firebase();
  if (!ready) throw new Error('Firebase가 설정되지 않았습니다.');
  const { doc, getDoc, setDoc } = await import('firebase/firestore');

  // 겹칠 일은 사실상 없지만, 겹치면 남의 팀에 들어가게 되므로 확인은 한다.
  let code = newCode();
  for (let tries = 0; tries < 3; tries++) {
    const existing = await getDoc(doc(ready.db, 'teams', code)).catch(() => null);
    if (!existing?.exists()) break;
    code = newCode();
  }

  await setDoc(doc(ready.db, 'teams', code), {
    name: name.trim(),
    createdBy: me.uid,
    createdAt: Date.now(),
  });
  await setDoc(doc(ready.db, 'teams', code, 'members', me.uid), { name: me.name, joinedAt: Date.now() });
  return { code, name: name.trim() };
}

/** 코드로 들어간다. 없는 코드면 `null`. */
export async function joinTeam(input: string, me: Person): Promise<Team | null> {
  const ready = await firebase();
  if (!ready) throw new Error('Firebase가 설정되지 않았습니다.');
  const code = cleanCode(input);
  if (code.length !== 8) return null;
  const { doc, getDoc, setDoc } = await import('firebase/firestore');

  // 오프라인 캐시가 아니라 서버에 묻는다 — "없는 코드"를 캐시에서 답하면 안 된다.
  const team = await getDoc(doc(ready.db, 'teams', code));
  if (!team.exists()) return null;
  await setDoc(doc(ready.db, 'teams', code, 'members', me.uid), { name: me.name, joinedAt: Date.now() });
  return { code, name: String(team.data()?.name ?? '') };
}

/** 이름을 바꾸면 팀의 구성원 목록에도 반영한다. 지난 쪽지에 적힌 이름은 그대로 둔다. */
export async function renameMember(code: string, me: Person): Promise<void> {
  const ready = await firebase();
  if (!ready) return;
  const { doc, setDoc } = await import('firebase/firestore');
  await setDoc(doc(ready.db, 'teams', code, 'members', me.uid), { name: me.name }, { merge: true });
}

export async function leaveTeam(code: string, uid: string): Promise<void> {
  const ready = await firebase();
  if (!ready) return;
  const { deleteDoc, doc } = await import('firebase/firestore');
  await deleteDoc(doc(ready.db, 'teams', code, 'members', uid)).catch(() => {});
}

/** 팀 구성원. 설정 화면에서 "누가 보고 있나"를 보여 준다. */
export async function watchMembers(code: string, listener: (members: Person[]) => void): Promise<() => void> {
  const ready = await firebase();
  if (!ready) return () => {};
  const { collection, onSnapshot } = await import('firebase/firestore');
  return onSnapshot(
    collection(ready.db, 'teams', code, 'members'),
    (snapshot) =>
      listener(
        snapshot.docs
          .map((entry) => ({ uid: entry.id, name: String(entry.data()?.name ?? '') }))
          .sort((a, b) => a.name.localeCompare(b.name, 'ko'))
      ),
    () => {}
  );
}

/* 쪽지 -------------------------------------------------------------- */

/**
 * 하루치 쪽지를 지켜본다.
 *
 * 날짜 하나로만 묻는다 — 같음 조건 하나는 Firestore가 색인 없이 답하므로, 콘솔에서 색인을
 * 만들라는 오류를 볼 일이 없다. 순서는 화면이 정한다(`sortSlips`).
 *
 * `fromServer`는 이 목록이 서버에서 확인된 것인지다. 새 쪽지 진동을 캐시에서 온 첫 목록에
 * 울리지 않으려고 쓴다.
 */
export async function watchSlips(
  code: string,
  date: string,
  listener: (slips: Slip[], added: Slip[], fromServer: boolean) => void,
  failed: (message: string) => void
): Promise<() => void> {
  const ready = await firebase();
  if (!ready) return () => {};
  const { collection, onSnapshot, query, where } = await import('firebase/firestore');
  return onSnapshot(
    query(collection(ready.db, 'teams', code, 'slips'), where('date', '==', date)),
    // 메타데이터 변화도 받는다. 캐시에서 온 목록과 서버의 목록이 같으면 그렇게 하지 않고는
    // "서버에서 확인됨"을 알 길이 없고, 그러면 새 쪽지 진동이 영영 켜지지 않는다.
    { includeMetadataChanges: true },
    (snapshot) => {
      const slips = snapshot.docs.map((entry) => toSlip(entry.id, entry.data()));
      const added = snapshot
        .docChanges()
        .filter((change) => change.type === 'added' && !change.doc.metadata.hasPendingWrites)
        .map((change) => toSlip(change.doc.id, change.doc.data()));
      listener(slips, added, !snapshot.metadata.fromCache);
    },
    (error) => failed(reason(error))
  );
}

/**
 * 새 쪽지. 쓰기는 기다리지 않는다.
 *
 * 오프라인이면 `setDoc`의 약속은 서버에 닿을 때까지 풀리지 않는다. 그걸 기다리면 신호가
 * 없는 곳에서 저장 버튼이 영영 돌아간다. 캐시에는 즉시 쓰이고 목록에도 즉시 뜨므로, 쓰기를
 * 걸어 두고 화면은 바로 닫는다.
 */
export async function addSlip(code: string, draft: SlipDraft, me: Person): Promise<string> {
  const ready = await firebase();
  if (!ready) throw new Error('Firebase가 설정되지 않았습니다.');
  const { collection, doc, setDoc } = await import('firebase/firestore');
  const ref = doc(collection(ready.db, 'teams', code, 'slips'));
  const now = Date.now();
  void setDoc(ref, {
    ...draft,
    status: 'waiting',
    assignee: null,
    createdBy: me,
    createdAt: now,
    updatedAt: now,
    updatedBy: me,
  }).catch(() => {});
  return ref.id;
}

export async function updateSlip(code: string, id: string, draft: Partial<SlipDraft>, me: Person): Promise<void> {
  const ready = await firebase();
  if (!ready) return;
  const { doc, updateDoc } = await import('firebase/firestore');
  void updateDoc(doc(ready.db, 'teams', code, 'slips', id), {
    ...draft,
    updatedAt: Date.now(),
    updatedBy: me,
  }).catch(() => {});
}

/**
 * 상태를 바꾼다.
 *
 * 맡으면 내 이름이 붙고, 대기로 되돌리면 이름이 떨어진다. 완료는 맡은 사람을 남긴다 —
 * 누가 모셨는지는 끝난 뒤에 더 자주 묻는다. 맡은 사람 없이 바로 완료를 누르면 누른 사람이
 * 맡은 것으로 한다.
 */
export async function setStatus(code: string, slip: Slip, status: Status, me: Person): Promise<void> {
  const ready = await firebase();
  if (!ready) return;
  const { doc, updateDoc } = await import('firebase/firestore');
  const assignee = status === 'waiting' ? null : status === 'doing' ? me : (slip.assignee ?? me);
  void updateDoc(doc(ready.db, 'teams', code, 'slips', slip.id), {
    status,
    assignee,
    updatedAt: Date.now(),
    updatedBy: me,
  }).catch(() => {});
}

export async function deleteSlip(code: string, id: string): Promise<void> {
  const ready = await firebase();
  if (!ready) return;
  const { deleteDoc, doc } = await import('firebase/firestore');
  void deleteDoc(doc(ready.db, 'teams', code, 'slips', id)).catch(() => {});
}

/**
 * 오래된 쪽지를 지운다.
 *
 * 서버에 따로 도는 작업이 없으므로, 팀의 누군가가 앱을 켤 때 그 폰이 치운다. 하루에
 * 몇 명은 켜므로 이걸로 충분하다. 실패해도 조용하다 — 다음에 켜는 사람이 치운다.
 */
export async function sweepOldSlips(code: string): Promise<number> {
  const ready = await firebase();
  if (!ready) return 0;
  try {
    const { collection, getDocs, limit, query, where, writeBatch } = await import('firebase/firestore');
    const cutoff = shiftDate(dateKey(), -RETENTION_DAYS + 1);
    const old = await getDocs(
      query(collection(ready.db, 'teams', code, 'slips'), where('date', '<', cutoff), limit(400))
    );
    if (old.empty) return 0;
    const batch = writeBatch(ready.db);
    old.docs.forEach((entry) => batch.delete(entry.ref));
    await batch.commit();
    return old.size;
  } catch {
    return 0;
  }
}
