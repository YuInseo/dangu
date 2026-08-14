import { useCallback, useEffect, useState } from 'react';

import {
  fetchGames,
  loadFirebaseConfig,
  pushGame,
  signInWithEmail,
  signInWithGoogle,
  signOut,
  watchAccount,
  watchGames,
  type Account,
  type SignInResult,
} from './firebase';
import { cloudChosen, loadHistory, mergeGames, recordGame } from './storage';
import { SCHEMA, type GameSummary } from './game';

/**
 * 로그인 상태와, 로그인했을 때만 일어나는 일들.
 *
 * 로그인은 이 앱에서 선택 사항이다. 계정 없이도 점수판은 완전히 동작하고, 로그인은
 * "이 폰이 아닌 곳에서도 기록을 본다"는 기능 하나를 켠다. 그래서 이 훅은 로그인되지
 * 않은 상태를 오류가 아니라 정상 상태로 취급한다.
 */
/** 마지막 맞춤이 어떻게 되었는지. 설정 화면이 한 줄로 적는다. */
export interface SyncState {
  state: 'idle' | 'syncing' | 'done' | 'error';
  /** 올린 판 수와 받은 판 수. */
  up: number;
  down: number;
  /** 끝난 시각. */
  at: number | null;
  reason?: string;
}

export interface AccountState {
  account: Account | null;
  /** 첫 확인이 끝나기 전. 이때 로그인 버튼을 그리면 로그인한 사람에게 한 번 깜빡인다. */
  loading: boolean;
  /** `null`은 아직 확인 중이라는 뜻이다. */
  configured: boolean | null;
  signingIn: boolean;
  error: string | null;
  signIn: () => Promise<void>;
  /**
   * 이메일로. `create`면 계정을 만든다.
   *
   * 구글 로그인과 나란히 있는 이유는 앱 안에서다 — 네이티브 구글 창은 APK 안의 설정을
   * 읽으므로 새 APK 없이는 켤 수 없고, 이쪽은 웹 번들만으로 동작한다.
   */
  signInEmail: (email: string, password: string, create: boolean) => Promise<void>;
  signOutNow: () => Promise<void>;
  /** 기록 맞추기의 상태. 자동으로 돌아가고, 이건 그 결과를 보는 자리다. */
  sync: SyncState;
  /** 손으로 한 번 더. 실패했거나, 방금 다른 기기에서 친 판을 당장 보고 싶을 때. */
  syncNow: () => Promise<void>;
}

/*
 * 맞춤은 앱 전체에 하나다.
 *
 * `useAccount()`는 로비에서도, 점수판에서도, 설정에서도 불린다. 각자가 자기 맞춤을
 * 돌리면 로그인 한 번에 같은 일이 세 번 일어나고, 그중 어느 것도 다른 화면에 보이지
 * 않는다. 그래서 상태를 모듈에 두고 화면들이 같은 것을 본다.
 */
let shared: SyncState = { state: 'idle', up: 0, down: 0, at: null };
const listeners = new Set<(next: SyncState) => void>();

function publish(next: SyncState) {
  shared = next;
  for (const listener of listeners) listener(next);
}

/** 이 실행에서 이미 맞춘 계정. 화면을 옮길 때마다 다시 맞추지 않으려고 기억한다. */
let settled: string | null = null;
let running: Promise<void> | null = null;

async function runSync(uid: string): Promise<void> {
  if (running) return running;
  publish({ ...shared, state: 'syncing' });
  running = (async () => {
    try {
      const report = await syncAll(uid);
      settled = uid;
      publish({ state: 'done', up: report.up, down: report.down, at: Date.now() });
    } catch (error: any) {
      // 실패는 기억하지 않는다 — 다음 화면에서 다시 해 본다. 당구장 지하에서 한 번
      // 실패한 것이 그날 내내 안 맞는 이유가 되어서는 안 된다.
      settled = null;
      publish({
        state: 'error',
        up: 0,
        down: 0,
        at: Date.now(),
        reason: error?.message ?? '맞추지 못했습니다.',
      });
    } finally {
      running = null;
    }
  })();
  return running;
}

export function useAccount(): AccountState {
  const [account, setAccount] = useState<Account | null>(null);
  const [loading, setLoading] = useState(true);
  const [signingIn, setSigningIn] = useState(false);
  const [error, setError] = useState<string | null>(null);
  /**
   * 설정이 있는지는 이제 비동기로 답한다.
   *
   * 빌드에 박힌 값 말고 사용자가 앱 안에서 넣은 값도 있기 때문이다 — 그건 기기 저장소에
   * 있으므로 읽는 데 한 틱이 걸린다. `null`은 "아직 모름"이고, 그 동안은 로그인 버튼도
   * 안내문도 그리지 않는다: 잠깐 나타났다 사라지는 안내문이 제일 나쁘다.
   */
  const [configured, setConfigured] = useState<boolean | null>(null);
  const [sync, setSync] = useState<SyncState>(shared);

  // 어느 화면에서 맞추든 모든 화면이 같은 상태를 본다.
  useEffect(() => {
    listeners.add(setSync);
    return () => {
      listeners.delete(setSync);
    };
  }, []);

  useEffect(() => {
    let cancelled = false;
    void loadFirebaseConfig().then((config) => {
      if (!cancelled) setConfigured(config !== null);
    });
    return () => {
      cancelled = true;
    };
  }, []);

  useEffect(() => {
    if (configured === null) return;
    if (!configured) {
      setLoading(false);
      return;
    }
    const stop = watchAccount((next) => {
      setAccount(next);
      setLoading(false);
    });
    return stop;
  }, [configured]);

  /*
   * 로그인해 있고 클라우드를 골랐으면, 묻지 않고 맞춘다.
   *
   * 예전에는 "기록 올리기"와 "기록 받기" 버튼이 설정에 있었다. 그런데 그 두 버튼을
   * 누를 시점을 아는 사람은 그걸 만든 사람뿐이다 — 폰을 바꿨을 때, 다른 기기에서
   * 쳤을 때, 지하에서 안 올라갔을 때. 어느 쪽이든 앱이 알 수 있는 일이고, 알 수 있는
   * 일을 사람에게 묻지 않는다.
   *
   * 한 실행에 한 번이면 충분하다. 판이 끝날 때마다 그 판은 따로 올라가므로, 여기서
   * 하는 일은 "빠진 것 채우기"다.
   */
  useEffect(() => {
    if (!account) return;
    void (async () => {
      if (!(await cloudChosen())) return;
      if (settled === account.uid) return;
      await runSync(account.uid);
    })();
  }, [account]);

  const syncNow = useCallback(async () => {
    if (!account) return;
    settled = null;
    await runSync(account.uid);
  }, [account]);

  /*
   * 클라우드를 계속 지켜본다.
   *
   * 맞추기가 "지금 빠진 것을 채우는" 일이라면 이쪽은 "그다음에 생기는 일"이다 — 다른
   * 폰에서 방금 끝낸 판은 이 폰이 아무것도 하지 않는 동안 생기므로, 물어보는 방식으로는
   * 언제 물어야 할지 알 수 없다. 붙어 있는 동안 밀어 주는 쪽을 쓴다.
   *
   * 화면을 옮기거나 로그아웃하면 끊는다. 남겨 두면 리스너가 쌓여 배터리와 읽기 할당량을
   * 계속 쓴다.
   */
  useEffect(() => {
    if (!account) return;
    let stop: (() => void) | null = null;
    let alive = true;

    void (async () => {
      if (!(await cloudChosen())) return;
      const off = await watchGames(account.uid, (remote) => {
        void absorb(account.uid, remote);
      });
      if (alive) stop = off;
      else off();
    })();

    return () => {
      alive = false;
      stop?.();
    };
  }, [account]);

  /** 로그인 결과 하나를 받아 상태에 반영한다. 어느 방법으로 들어왔든 그다음은 같다. */
  const settle = useCallback((result: SignInResult) => {
    setSigningIn(false);
    if (result.ok) {
      setAccount(result.account);
      // 방금 들어온 계정으로 곧바로 맞춘다. 위의 효과도 같은 일을 하지만, 로그인 직후는
      // 사람이 결과를 보고 있는 순간이라 한 틱이라도 빠른 편이 낫다.
      void cloudChosen().then((yes) => {
        if (yes) void runSync(result.account.uid);
      });
    } else if (!result.cancelled) {
      setError(result.reason);
    }
  }, []);

  const signIn = useCallback(async () => {
    setSigningIn(true);
    setError(null);
    settle(await signInWithGoogle());
  }, [settle]);

  const signInEmail = useCallback(
    async (email: string, password: string, create: boolean) => {
      setSigningIn(true);
      setError(null);
      settle(await signInWithEmail(email, password, create));
    },
    [settle]
  );

  const signOutNow = useCallback(async () => {
    await signOut();
    setAccount(null);
  }, []);

  return {
    account,
    loading,
    configured,
    signingIn,
    error,
    signIn,
    signInEmail,
    signOutNow,
    sync,
    syncNow,
  };
}

/**
 * 클라우드에서 밀려온 것을 받아 넣는다.
 *
 * 세 가지를 한다. 기기에 없거나 더 오래된 판을 갈아 끼우고, 클라우드에 없는 판을 올리고,
 * 옛 모양으로 저장된 판을 지금 모양으로 다시 올린다. 마지막 것이 이 앱의 판올림이다 —
 * 새 앱이 붙는 순간 그 계정의 기록이 조용히 최신 모양이 되고, 아직 옛 앱을 쓰는 폰은
 * 자기가 아는 값만 읽으면 되므로 아무 일도 일어나지 않는다.
 *
 * 올린 것은 다시 스냅숏으로 돌아오지만, 그때는 바뀐 것이 없어 아무 일도 하지 않는다.
 */
async function absorb(uid: string, remote: GameSummary[]): Promise<void> {
  const before = await loadHistory();
  const merged = await mergeGames(remote);

  const there = new Map(remote.map((game) => [game.id, game]));
  const now = Date.now();

  for (const game of merged) {
    const cloud = there.get(game.id);

    // 클라우드에 없는 판 — 이 폰에서만 친 것이다.
    if (!cloud) {
      await pushGame(uid, game);
      continue;
    }

    // 옛 모양으로 저장되어 있던 판. 옮긴 값을 되돌려 준다.
    if ((cloud.schema ?? 0) < SCHEMA) {
      await pushGame(uid, { ...game, updatedAt: game.updatedAt ?? now });
      continue;
    }

    // 이 폰의 것이 더 새것이면 올린다.
    if ((game.updatedAt ?? 0) > (cloud.updatedAt ?? 0)) await pushGame(uid, game);
  }

  // 목록이 실제로 달라졌을 때만 화면에 알린다.
  if (merged !== before) publish({ ...shared, state: 'done', at: Date.now() });
}

export interface SyncReport {
  up: number;
  down: number;
  games: GameSummary[];
}

/**
 * 양쪽을 한 번에 맞춘다 — 없는 것만 주고받는다.
 *
 * 기기에 없는 것은 내려받고, 클라우드에 없는 것은 올린다. 양쪽에 다 있는 판은 건드리지
 * 않는다: 같은 값을 쓰는 것도 쓰기이고, 판 오십 개를 앱 켤 때마다 다시 올릴 이유가 없다.
 * 이미 있는 판의 내용이 바뀌는 경우(기록 고치기)는 고치는 자리에서 그 판만 올린다.
 */
export async function syncAll(uid: string): Promise<SyncReport> {
  const [remote, local] = await Promise.all([fetchGames(uid), loadHistory()]);
  const here = new Set(local.map((entry) => entry.id));
  const there = new Set(remote.map((entry) => entry.id));

  let down = 0;
  for (const game of remote) {
    if (here.has(game.id)) continue;
    await recordGame(game);
    down++;
  }

  let up = 0;
  for (const game of local) {
    if (there.has(game.id)) continue;
    if (await pushGame(uid, game)) up++;
  }

  return { up, down, games: await loadHistory() };
}

/** 기기에 있는 기록을 클라우드로. 이미 올라간 건 같은 문서에 덮어써도 값이 같다. */
export async function syncUp(uid: string): Promise<number> {
  const local = await loadHistory();
  let uploaded = 0;
  for (const game of local) {
    if (await pushGame(uid, game)) uploaded++;
  }
  return uploaded;
}

/**
 * 클라우드에 있는데 기기에 없는 기록을 내려받는다.
 *
 * 폰을 바꾼 사람이 로그인하면 이쪽이 도는 경로다. 겹치는 건 id로 걸러지므로 두 번
 * 세지 않는다.
 */
export async function syncDown(uid: string): Promise<GameSummary[]> {
  const [remote, local] = await Promise.all([fetchGames(uid), loadHistory()]);
  const known = new Set(local.map((entry) => entry.id));
  for (const game of remote) {
    if (!known.has(game.id)) await recordGame(game);
  }
  return loadHistory();
}
