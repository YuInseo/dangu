/**
 * 서버 쪽 설정. 기기마다 다른 것(DSP, 화면 취향)은 브라우저에, 라이브러리 전체에 걸리는 것만 여기.
 */
import fs from 'node:fs/promises';
import path from 'node:path';
import { config } from '../config';

export interface ServerSettings {
  /** 백그라운드 오디오 분석 속도. 룬의 "Background audio analysis speed". */
  analysisSpeed: 'off' | 'throttled' | 'fast';
}

const DEFAULTS: ServerSettings = { analysisSpeed: 'throttled' };
const file = () => path.join(config.dataDir, 'settings.json');

const g = globalThis as typeof globalThis & { __soriSettings?: ServerSettings };

export async function getSettings(): Promise<ServerSettings> {
  if (g.__soriSettings) return g.__soriSettings;
  try {
    g.__soriSettings = { ...DEFAULTS, ...(JSON.parse(await fs.readFile(file(), 'utf8')) as Partial<ServerSettings>) };
  } catch {
    g.__soriSettings = { ...DEFAULTS };
  }
  return g.__soriSettings;
}

export async function updateSettings(patch: Partial<ServerSettings>): Promise<ServerSettings> {
  const next = { ...(await getSettings()), ...patch };
  if (!['off', 'throttled', 'fast'].includes(next.analysisSpeed)) next.analysisSpeed = DEFAULTS.analysisSpeed;
  g.__soriSettings = next;
  await fs.mkdir(config.dataDir, { recursive: true });
  await fs.writeFile(file(), JSON.stringify(next, null, 1));
  return next;
}
