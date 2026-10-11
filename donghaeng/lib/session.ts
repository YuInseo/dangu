'use client';

import { useCallback, useEffect, useState } from 'react';

import { ensureUser, reason, type Team } from './firebase';
import { preferencesGet, preferencesSet } from './platform';
import type { Person } from './slip';

/**
 * 이 폰이 누구이고 어느 팀에 있는지.
 *
 * 셋 다 있어야 쪽지판이 뜬다: 익명 uid(앱이 받는다), 이름(사람이 적는다), 팀(만들거나
 * 코드로 들어간다). 이름과 팀은 기기에 남겨서 다음에 켤 때 묻지 않는다.
 */

const NAME_KEY = 'donghaeng.name';
const TEAM_KEY = 'donghaeng.team';

export interface Session {
  loading: boolean;
  error: string | null;
  uid: string | null;
  name: string;
  team: Team | null;
  /** 쪽지에 찍히는 나. uid가 없으면 `null`. */
  me: Person | null;
  setName: (name: string) => Promise<void>;
  setTeam: (team: Team | null) => Promise<void>;
  retry: () => void;
}

// 화면을 옮겨도 다시 읽지 않게 한 벌만 둔다.
let memory: { uid: string | null; name: string; team: Team | null } | null = null;

export function useSession(): Session {
  const [state, setState] = useState(memory ?? { uid: null, name: '', team: null });
  const [loading, setLoading] = useState(!memory);
  const [error, setError] = useState<string | null>(null);
  const [attempt, setAttempt] = useState(0);

  useEffect(() => {
    if (memory?.uid) return;
    let alive = true;
    void (async () => {
      const [name, teamText] = await Promise.all([preferencesGet(NAME_KEY), preferencesGet(TEAM_KEY)]);
      let team: Team | null = null;
      try {
        team = teamText ? JSON.parse(teamText) : null;
      } catch {
        team = null;
      }
      try {
        const uid = await ensureUser();
        if (!alive) return;
        memory = { uid, name: name ?? '', team };
        setState(memory);
        setError(null);
      } catch (failure) {
        if (!alive) return;
        // 로그인이 안 돼도 이름과 팀은 보여 준다. 무엇이 막혔는지는 오류 줄이 말한다.
        setState({ uid: null, name: name ?? '', team });
        setError(reason(failure));
      } finally {
        if (alive) setLoading(false);
      }
    })();
    return () => {
      alive = false;
    };
  }, [attempt]);

  const setName = useCallback(async (name: string) => {
    const clean = name.trim();
    await preferencesSet(NAME_KEY, clean);
    setState((current) => (memory = { ...current, name: clean }));
  }, []);

  const setTeam = useCallback(async (team: Team | null) => {
    await preferencesSet(TEAM_KEY, team ? JSON.stringify(team) : '');
    setState((current) => (memory = { ...current, team }));
  }, []);

  const retry = useCallback(() => {
    memory = null;
    setLoading(true);
    setAttempt((value) => value + 1);
  }, []);

  return {
    loading,
    error,
    ...state,
    me: state.uid && state.name ? { uid: state.uid, name: state.name } : null,
    setName,
    setTeam,
    retry,
  };
}
