import { useCallback, useEffect, useState } from 'react';

import {
  reason,
  signInAnonymously,
  signInWithGoogle,
  signOut,
  watchAccount,
  type Account,
  type Team,
} from './firebase';
import { preferencesGet, preferencesSet } from './platform';
import type { Person } from './slip';

/**
 * 이 폰이 누구이고 어느 팀에 있는지.
 *
 * 셋 다 있어야 쪽지판이 뜬다: 구글 계정, 쪽지에 붙을 이름, 팀. 이름은 구글 이름으로
 * 채워 두되 고칠 수 있다 — 구글 이름이 영어거나 별명인 사람이 많다. 이름과 팀은 기기에
 * 남겨서 다음에 켤 때 묻지 않는다.
 */

const NAME_KEY = 'donghaeng.name';
const TEAM_KEY = 'donghaeng.team';

export interface Session {
  loading: boolean;
  error: string | null;
  account: Account | null;
  uid: string | null;
  name: string;
  team: Team | null;
  /** 쪽지에 찍히는 나. 로그인 전이면 `null`. */
  me: Person | null;
  signingIn: boolean;
  signIn: () => Promise<void>;
  signInAnonymously: () => Promise<void>;
  signOut: () => Promise<void>;
  setName: (name: string) => Promise<void>;
  setTeam: (team: Team | null) => Promise<void>;
}

export function useSession(): Session {
  const [account, setAccount] = useState<Account | null>(null);
  const [checked, setChecked] = useState(false);
  const [stored, setStored] = useState<{ name: string; team: Team | null } | null>(null);
  const [signingIn, setSigningIn] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let alive = true;
    void (async () => {
      const [name, teamText] = await Promise.all([preferencesGet(NAME_KEY), preferencesGet(TEAM_KEY)]);
      let team: Team | null = null;
      try {
        team = teamText ? JSON.parse(teamText) : null;
      } catch {
        team = null;
      }
      if (alive) setStored({ name: name ?? '', team });
    })();
    const stop = watchAccount((next) => {
      if (!alive) return;
      setAccount(next);
      setChecked(true);
    });
    return () => {
      alive = false;
      stop();
    };
  }, []);

  const setName = useCallback(async (name: string) => {
    const clean = name.trim();
    await preferencesSet(NAME_KEY, clean);
    setStored((current) => ({ team: current?.team ?? null, name: clean }));
  }, []);

  const setTeam = useCallback(async (team: Team | null) => {
    await preferencesSet(TEAM_KEY, team ? JSON.stringify(team) : '');
    setStored((current) => ({ name: current?.name ?? '', team }));
  }, []);

  const attempt = useCallback(async (work: () => Promise<unknown>) => {
    setSigningIn(true);
    setError(null);
    try {
      await work();
    } catch (failure) {
      setError(reason(failure));
    } finally {
      setSigningIn(false);
    }
  }, []);

  const signIn = useCallback(() => attempt(signInWithGoogle), [attempt]);
  const signInAnon = useCallback(() => attempt(signInAnonymously), [attempt]);

  const signOutNow = useCallback(async () => {
    await signOut();
  }, []);

  // 적어 둔 이름이 없으면 구글 이름을 쓴다.
  const name = stored?.name || account?.name || '';

  return {
    loading: !checked || !stored,
    error,
    account,
    uid: account?.uid ?? null,
    name,
    team: stored?.team ?? null,
    me: account && name ? { uid: account.uid, name } : null,
    signingIn,
    signIn,
    signInAnonymously: signInAnon,
    signOut: signOutNow,
    setName,
    setTeam,
  };
}
