import { useCallback, useEffect, useState } from 'react';

import {
  deleteGame,
  fetchGames,
  loadFirebaseConfig,
  pushGame,
  pushPrefs,
  pushTrash,
  signInWithEmail,
  signInWithGoogle,
  signOut,
  watchAccount,
  watchGames,
  watchPrefs,
  watchTrash,
  type Account,
  type CloudPrefs,
  type SignInResult,
} from './firebase';
import {
  applyRemoteSettings,
  applyTrash,
  joinRemoteSettings,
  loadHistory,
  loadSettings,
  loadTrash,
  mergeGames,
  recordGame,
  saveSettings,
  sharedSettings,
  watchSettings,
  type AppSettings,
  type SyncMode,
  type Trash,
} from './storage';
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
  /**
   * 이번 로그인에서 저장 방식을 계정으로 옮겼는지.
   *
   * 앱이 스스로 한 일이라 사람에게 한 번은 말해야 한다 — 설명 없이 저장 위치가 바뀌면
   * 그건 편의가 아니라 놀랄 일이다.
   */
  migrated?: boolean;
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

/**
 * 이 기기를 계정에 맞출 상태인지.
 *
 * 클라우드를 쓰고 있거나, 아직 아무것도 고르지 않았거나 — 뒤엣것이 옮겨질 자리다.
 * 직접 "이 기기에만"을 고른 사람만 여기서 걸러진다.
 */
async function linked(): Promise<boolean> {
  const settings = await loadSettings();
  return settings.sync === 'cloud' || !settings.syncPinned;
}

/**
 * 로그인했으면 계정에 둔다 — 묻지 않고.
 *
 * 예전에는 로그인과 저장 방식이 따로였다. 로그인해 놓고도 설정에서 "구글 계정"을 다시
 * 고르지 않으면 아무것도 올라가지 않았고, 그 사실은 화면 어디에도 크게 적혀 있지 않았다.
 * 두 번째 기기에서 로그인한 사람이 기록이 비어 있는 것을 보고 "동기화가 안 된다"고 하는
 * 것이 정확히 이 자리다 — 그 사람은 이미 자기 뜻을 말했다. 계정으로 로그인한 것이 그
 * 뜻이다.
 *
 * 다만 직접 "이 기기에만"을 고른 사람의 결정은 뒤집지 않는다. 그래서 기본값으로서의
 * `local`과 사람이 고른 `local`을 `syncPinned`로 가른다.
 */
async function migrateToCloud(): Promise<boolean> {
  const settings = await loadSettings();
  if (settings.sync === 'cloud') return false;
  if (settings.syncPinned) return false;
  await saveSettings({ ...settings, sync: 'cloud' });
  return true;
}

async function runSync(uid: string): Promise<void> {
  if (running) return running;
  publish({ ...shared, state: 'syncing' });
  running = (async () => {
    try {
      const migrated = await migrateToCloud();
      const report = await syncAll(uid);
      settled = uid;
      publish({ state: 'done', up: report.up, down: report.down, at: Date.now(), migrated });
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
   * 저장 방식을 지켜본다.
   *
   * 예전에는 이 값을 효과가 시작될 때 한 번 읽었다. 그러면 설정에서 "구글 계정"으로
   * 바꿔도 지켜보기가 시작되지 않는다 — 다른 기기에서 친 판이 들어오는 길이 그때부터
   * 열려야 하는데, 앱을 다시 켤 때까지 닫혀 있었다. 켜고 끄는 값이면 켜지는 순간을
   * 알아야 한다.
   */
  const [mode, setMode] = useState<SyncMode | null>(null);

  useEffect(() => {
    let alive = true;
    void loadSettings().then((settings) => {
      if (alive) setMode(settings.sync);
    });
    const stop = watchSettings((settings) => setMode(settings.sync));
    return () => {
      alive = false;
      stop();
    };
  }, []);

  /*
   * 로그인해 있으면, 묻지 않고 맞춘다.
   *
   * 예전에는 "기록 올리기"와 "기록 받기" 버튼이 설정에 있었다. 그런데 그 두 버튼을
   * 누를 시점을 아는 사람은 그걸 만든 사람뿐이다 — 폰을 바꿨을 때, 다른 기기에서
   * 쳤을 때, 지하에서 안 올라갔을 때. 어느 쪽이든 앱이 알 수 있는 일이고, 알 수 있는
   * 일을 사람에게 묻지 않는다.
   *
   * 이제 저장 방식을 고르는 일도 그 목록에 들어간다. 계정으로 로그인한 사람은 이미
   * 자기 뜻을 말했으므로 `runSync`가 그 자리에서 옮긴다 — 직접 "이 기기에만"을 고른
   * 사람만 예외다.
   *
   * 한 실행에 한 번이면 충분하다. 판이 끝날 때마다 그 판은 따로 올라가므로, 여기서
   * 하는 일은 "빠진 것 채우기"다.
   */
  useEffect(() => {
    if (!account) return;
    void (async () => {
      if (!(await linked())) return;
      if (settled === account.uid) return;
      await runSync(account.uid);
    })();
  }, [account, mode]);

  const syncNow = useCallback(async () => {
    if (!account) return;
    settled = null;
    await runSync(account.uid);
  }, [account]);

  /*
   * 클라우드를 계속 지켜본다 — 기록도, 지운 것도, 설정도.
   *
   * 맞추기가 "지금 빠진 것을 채우는" 일이라면 이쪽은 "그다음에 생기는 일"이다 — 다른
   * 폰에서 방금 끝낸 판은 이 폰이 아무것도 하지 않는 동안 생기므로, 물어보는 방식으로는
   * 언제 물어야 할지 알 수 없다. 붙어 있는 동안 밀어 주는 쪽을 쓴다.
   *
   * 화면을 옮기거나 로그아웃하면 끊는다. 남겨 두면 리스너가 쌓여 배터리와 읽기 할당량을
   * 계속 쓴다.
   */
  useEffect(() => {
    if (!account || mode !== 'cloud') return;
    let stop: (() => void) | null = null;
    let alive = true;

    void (async () => {
      const off = await link(account.uid);
      if (alive) stop = off;
      else off();
    })();

    return () => {
      alive = false;
      stop?.();
    };
  }, [account, mode]);

  /** 로그인 결과 하나를 받아 상태에 반영한다. 어느 방법으로 들어왔든 그다음은 같다. */
  const settle = useCallback((result: SignInResult) => {
    setSigningIn(false);
    if (result.ok) {
      setAccount(result.account);
      // 방금 들어온 계정으로 곧바로 맞춘다. 위의 효과도 같은 일을 하지만, 로그인 직후는
      // 사람이 결과를 보고 있는 순간이라 한 틱이라도 빠른 편이 낫다.
      void linked().then((yes) => {
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
  const trash = await loadTrash();

  const there = new Map(remote.map((game) => [game.id, game]));
  const now = Date.now();

  /*
   * 여기서 지운 판이 클라우드에 아직 있는 경우.
   *
   * 지운 사이에 다른 기기가 그 판을 도로 올렸을 때 생긴다 — 그쪽은 아직 이름표를 못
   * 받았고, 자기가 아는 것은 "클라우드에 없는 판"뿐이었다. 이름표는 곧 그쪽에도 닿아
   * 다시 올리는 일이 멈추지만, 이미 올라간 문서는 누군가 치워야 한다.
   */
  for (const game of remote) {
    const at = trash[game.id] ?? 0;
    if (at >= (game.updatedAt ?? 0)) await deleteGame(uid, game.id, at);
  }

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

/* 지켜보기 ---------------------------------------------------------- */

/**
 * 이 기기를 계정에 붙인다 — 기록, 지운 것, 설정 셋 다.
 *
 * 셋을 한 자리에 모아 둔 이유는 수명이 같기 때문이다. 붙는 순간도 끊는 순간도 같고,
 * 하나만 붙어 있는 상태는 어느 쪽으로든 반쪽짜리다 — 기록만 흐르면 다른 기기에서 지운
 * 판이 되살아나고, 지운 것만 흐르면 아무것도 오지 않는다.
 */
async function link(uid: string): Promise<() => void> {
  const stops = await Promise.all([
    watchGames(uid, (remote) => {
      void absorb(uid, remote);
    }),
    linkTrash(uid),
    linkPrefs(uid),
  ]);
  return () => {
    for (const stop of stops) stop();
  };
}

/** 이름표를 한 번에 이만큼까지 본다. `watchTrash`의 한도와 같은 값이어야 한다. */
const TRASH_PAGE = 500;

/**
 * 지운 판을 양쪽으로 흘린다.
 *
 * 받는 쪽은 간단하다 — 저쪽에서 지운 것을 여기서도 지운다. 보내는 쪽은 스냅숏을 그대로
 * 쓴다: 클라우드에 없는 이름표가 곧 아직 안 알린 것이므로, 따로 "무엇을 올렸는지"를
 * 기억할 필요가 없다. 올리고 나면 그것이 스냅숏으로 돌아오고, 그때는 올릴 것이 없다.
 */
async function linkTrash(uid: string): Promise<() => void> {
  return watchTrash(
    uid,
    (remote) => {
      void (async () => {
        const removed = await applyTrash(remote);
        const mine = await loadTrash();

        /*
         * 스냅숏이 한도까지 찼으면 그보다 오래된 이름표가 저쪽에 있는지 알 수 없다.
         * 모르는 것을 올리면 스냅숏이 올 때마다 같은 이름표를 다시 쓰게 되므로, 보이는
         * 범위 안의 것만 다룬다. 그 바깥은 이미 오래전에 할 일이 끝난 이름표들이다.
         */
        const floor =
          Object.keys(remote).length >= TRASH_PAGE ? Math.min(...Object.values(remote)) : 0;

        /*
         * 저쪽이 모르거나, 저쪽이 아는 것보다 나중에 지운 것.
         *
         * 뒤엣것이 필요한 이유는 되살아나는 판이 있어서다 — 끝낸 판을 되돌려 다시 치면
         * 그 판은 새 시각으로 돌아오고, 나중에 다시 지우면 이름표도 새 시각이어야 한다.
         * 옛 시각이 남아 있으면 그 이름표는 아무 일도 하지 못한다.
         */
        const missing: Trash = {};
        for (const [id, at] of Object.entries(mine)) {
          if ((remote[id] ?? 0) < at && at >= floor) missing[id] = at;
        }
        await pushTrash(uid, missing);

        if (removed) publish({ ...shared, state: 'done', at: Date.now() });
      })();
    },
    TRASH_PAGE
  );
}

/**
 * 클라우드에 올렸거나 클라우드에서 받은 설정의 시각.
 *
 * 되돌아온 것을 다시 올리지 않으려고 둔다. 이 값이 없으면 두 기기가 같은 설정을 서로에게
 * 영원히 밀어 준다 — 받은 값을 저장하는 것도 저장이고, 저장은 올리기를 부르기 때문이다.
 */
let prefsAt = 0;

/**
 * 설정을 양쪽으로 흘린다.
 *
 * 처음 붙는 순간만 합치고, 그다음부터는 나중에 고친 쪽이 이긴다 — 왜 그 둘이 달라야
 * 하는지는 `joinRemoteSettings`에 적어 두었다.
 */
async function linkPrefs(uid: string): Promise<() => void> {
  // 계정이 바뀌었을 수 있다. 앞 계정의 시각으로 이 계정의 첫 올리기를 막으면 안 된다.
  prefsAt = 0;
  let joined = false;

  const send = async (settings: AppSettings) => {
    const at = settings.updatedAt ?? 0;
    if (at <= prefsAt) return;
    prefsAt = at;
    await pushPrefs(uid, sharedSettings(settings));
  };

  const stopLocal = watchSettings((settings) => void send(settings));

  const stopRemote = await watchPrefs(uid, (remote: CloudPrefs | null) => {
    void (async () => {
      if (!joined) {
        joined = true;
        const { settings, changed } = await joinRemoteSettings(remote);
        // 합쳐서 새로 생긴 값이면 도장을 찍는다. 저장이 지켜보는 쪽을 깨우고, 그 길로
        // 올라간다 — 올리는 자리를 한 곳에만 두려고 이렇게 한다.
        if (changed) await saveSettings(settings);
        else prefsAt = settings.updatedAt ?? 0;
        return;
      }

      if (!remote) return send(await loadSettings());

      prefsAt = Math.max(prefsAt, remote.updatedAt ?? 0);
      const applied = await applyRemoteSettings(remote);
      // 받을 것이 없었다면 이 기기 쪽이 더 새것이다. 그러면 반대로 올라간다.
      if (!applied) await send(await loadSettings());
    })();
  });

  return () => {
    stopLocal();
    stopRemote();
  };
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
 *
 * "기기에 없는 것"에는 두 가지가 섞여 있다 — 아직 못 받은 것과 여기서 지운 것. 뒤엣것을
 * 받으면 지우기가 되돌려지므로 이름표를 보고 가른다.
 */
export async function syncAll(uid: string): Promise<SyncReport> {
  const [remote, local, trash] = await Promise.all([fetchGames(uid), loadHistory(), loadTrash()]);
  const here = new Set(local.map((entry) => entry.id));
  const there = new Set(remote.map((entry) => entry.id));

  let down = 0;
  for (const game of remote) {
    if (here.has(game.id)) continue;
    if ((trash[game.id] ?? 0) >= (game.updatedAt ?? 0)) continue;
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
