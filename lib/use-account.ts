import { useCallback, useEffect, useState } from 'react';

import {
  fetchGames,
  loadFirebaseConfig,
  pushGame,
  signInWithGoogle,
  signOut,
  watchAccount,
  type Account,
} from './firebase';
import { cloudChosen, loadHistory, recordGame } from './storage';
import type { GameSummary } from './game';

/**
 * 로그인 상태와, 로그인했을 때만 일어나는 일들.
 *
 * 로그인은 이 앱에서 선택 사항이다. 계정 없이도 점수판은 완전히 동작하고, 로그인은
 * "이 폰이 아닌 곳에서도 기록을 본다"는 기능 하나를 켠다. 그래서 이 훅은 로그인되지
 * 않은 상태를 오류가 아니라 정상 상태로 취급한다.
 */
export interface AccountState {
  account: Account | null;
  /** 첫 확인이 끝나기 전. 이때 로그인 버튼을 그리면 로그인한 사람에게 한 번 깜빡인다. */
  loading: boolean;
  /** `null`은 아직 확인 중이라는 뜻이다. */
  configured: boolean | null;
  signingIn: boolean;
  error: string | null;
  signIn: () => Promise<void>;
  signOutNow: () => Promise<void>;
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

  const signIn = useCallback(async () => {
    setSigningIn(true);
    setError(null);
    const result = await signInWithGoogle();
    setSigningIn(false);
    if (result.ok) {
      setAccount(result.account);
      // 클라우드 저장을 고른 사람에 한해, 이 폰에만 있던 기록을 올린다. 로그인은
      // 했지만 저장 방식은 '이 기기에만'인 사람의 데이터를 대신 올려 주지 않는다.
      void cloudChosen().then((yes) => {
        if (yes) void syncUp(result.account.uid);
      });
    } else if (!result.cancelled) {
      setError(result.reason);
    }
  }, []);

  const signOutNow = useCallback(async () => {
    await signOut();
    setAccount(null);
  }, []);

  return { account, loading, configured, signingIn, error, signIn, signOutNow };
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
