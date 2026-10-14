/**
 * 서버와 라이브러리의 지금 상태. 처음에 한 번 받고, 그 뒤로는 서버가 알려 주는 대로 바꾼다.
 */
import { createContext, useContext, useEffect, useState, type ReactNode } from 'react';
import { fetchJson, noteGeneration } from '@/api';
import type { AnalysisStatus, LibrarySummary, ScanStatus } from '@/lib/library/types';

export interface LibraryInfo {
  summary: LibrarySummary;
  status: ScanStatus;
  analysis: AnalysisStatus;
  generation: number;
  musicDir: string;
  dataDir: string;
  version: string;
  auth: boolean;
  platform: string;
}

const Ctx = createContext<LibraryInfo | null>(null);

export function LibraryProvider({ children, fallback }: { children: ReactNode; fallback: ReactNode }) {
  const [info, setInfo] = useState<LibraryInfo | null>(null);

  useEffect(() => {
    let alive = true;
    const load = () =>
      fetchJson<LibraryInfo>('/api/library')
        .then((i) => {
          if (!alive) return;
          noteGeneration(i.generation);
          setInfo(i);
        })
        .catch(() => {
          if (alive) setTimeout(load, 2000);
        });
    void load();
    const es = new EventSource('/api/events');
    es.addEventListener('library', (e) => {
      const d = JSON.parse((e as MessageEvent<string>).data) as Pick<LibraryInfo, 'generation' | 'status' | 'analysis'>;
      setInfo((prev) => {
        if (prev && prev.generation !== d.generation) void load();
        return prev ? { ...prev, status: d.status, analysis: d.analysis } : prev;
      });
      noteGeneration(d.generation);
    });
    return () => {
      alive = false;
      es.close();
    };
  }, []);

  if (!info) return <>{fallback}</>;
  return <Ctx.Provider value={info}>{children}</Ctx.Provider>;
}

export function useLibraryInfo(): LibraryInfo {
  const v = useContext(Ctx);
  if (!v) throw new Error('LibraryProvider 밖입니다');
  return v;
}
