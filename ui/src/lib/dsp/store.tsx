'use client';

/**
 * 이 기기(존)의 DSP 설정과 프리셋. 브라우저에 저장된다 — 룬도 DSP는 존마다 따로다.
 */
import { createContext, useCallback, useContext, useEffect, useMemo, useRef, useState, type ReactNode } from 'react';
import { readJSON, writeJSON } from '../player/storage';
import { defaultConfig, newFilter } from './defaults';
import type { DspConfig, DspFilter } from './types';

export interface Preset {
  name: string;
  config: DspConfig;
  savedAt: number;
}

interface DspStore {
  config: DspConfig;
  /** 복사본을 고쳐서 돌려준다. 바뀐 것만 다시 그린다. */
  update(fn: (draft: DspConfig) => void): void;
  replace(config: DspConfig): void;
  presets: Preset[];
  /** 지금 설정이 어느 프리셋에서 왔는지. 손대면 modified가 된다. */
  preset: { name: string; modified: boolean } | null;
  savePreset(name: string): void;
  loadPreset(name: string): void;
  deletePreset(name: string): void;
  reset(): void;
}

const Ctx = createContext<DspStore | null>(null);

export function useDsp(): DspStore {
  const v = useContext(Ctx);
  if (!v) throw new Error('DspProvider 밖');
  return v;
}

/** 예전에 저장된 설정에 새 항목이 빠져 있어도 기본값으로 채운다. */
export function normalize(raw: unknown): DspConfig {
  const d = defaultConfig();
  if (!raw || typeof raw !== 'object' || (raw as DspConfig).v !== 1) return d;
  const r = raw as Partial<DspConfig>;
  const filters = Array.isArray(r.filters)
    ? (r.filters.filter((f) => f && typeof f === 'object' && 'type' in f && 'id' in f) as DspFilter[]).map(
        (f) => ({ ...newFilter(f.type), ...f }) as DspFilter,
      )
    : d.filters;
  return {
    v: 1,
    enabled: !!r.enabled,
    headroom: { ...d.headroom, ...r.headroom },
    src: { ...d.src, ...r.src, custom: { ...(r.src?.custom ?? {}) } },
    leveling: { ...d.leveling, ...r.leveling },
    output: { ...d.output, ...r.output },
    bypass: !!r.bypass,
    filters,
  };
}

const KEY = 'sori.dsp';
const PRESETS = 'sori.dsp.presets';
const ACTIVE = 'sori.dsp.preset';

export function DspProvider({ children }: { children: ReactNode }) {
  const [config, setConfig] = useState<DspConfig>(defaultConfig);
  const [presets, setPresets] = useState<Preset[]>([]);
  const [preset, setPreset] = useState<DspStore['preset']>(null);
  const loaded = useRef(false);

  useEffect(() => {
    setConfig(normalize(readJSON(KEY)));
    setPresets((readJSON<Preset[]>(PRESETS) ?? []).map((p) => ({ ...p, config: normalize(p.config) })));
    setPreset(readJSON(ACTIVE));
    loaded.current = true;
  }, []);

  useEffect(() => {
    if (loaded.current) writeJSON(KEY, config);
  }, [config]);
  useEffect(() => {
    if (loaded.current) writeJSON(PRESETS, presets);
  }, [presets]);
  useEffect(() => {
    if (loaded.current) writeJSON(ACTIVE, preset);
  }, [preset]);

  const update = useCallback((fn: (draft: DspConfig) => void) => {
    setConfig((c) => {
      const next = structuredClone(c);
      fn(next);
      return next;
    });
    setPreset((p) => (p && !p.modified ? { ...p, modified: true } : p));
  }, []);

  const store = useMemo<DspStore>(
    () => ({
      config,
      update,
      replace(c) {
        setConfig(normalize(c));
      },
      presets,
      preset,
      savePreset(name) {
        const n = name.trim().slice(0, 40);
        if (!n) return;
        setPresets((ps) => [...ps.filter((p) => p.name !== n), { name: n, config: structuredClone(config), savedAt: Date.now() }]);
        setPreset({ name: n, modified: false });
      },
      loadPreset(name) {
        const p = presets.find((x) => x.name === name);
        if (!p) return;
        setConfig(normalize(structuredClone(p.config)));
        setPreset({ name, modified: false });
      },
      deletePreset(name) {
        setPresets((ps) => ps.filter((p) => p.name !== name));
        setPreset((p) => (p?.name === name ? null : p));
      },
      reset() {
        setConfig(defaultConfig());
        setPreset(null);
      },
    }),
    [config, update, presets, preset],
  );

  return <Ctx.Provider value={store}>{children}</Ctx.Provider>;
}
