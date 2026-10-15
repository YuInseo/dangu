/**
 * 다른 기기와 잇기(선택). 이 기기는 혼자서도 다 된다. 연결은 화면을 그 소리로 옮기는 것이고,
 * 옮겨 간 화면에는 "돌아가기"가 남는다.
 */
import { fetchJson } from '@/api';

export interface Saved {
  name: string;
  url: string;
}

export interface LinkInfo {
  id: string;
  name: string;
  /** off: 이 기기 안에서만, on: 같은 와이파이에 열어 둠, always: 처음부터 열린 서버 */
  mode: 'off' | 'on' | 'always';
  port: number | null;
  urls: string[];
  /** 이 기기에서 볼 때만, 공유를 켰고 비밀번호가 없을 때 */
  pin: string | null;
  password: boolean;
  /** 이 기기 안에서 연 화면인가(다른 기기에서 연 화면이면 공유 설정을 못 바꾼다) */
  local: boolean;
  saved: Saved[];
}

export interface Found {
  id: string;
  name: string;
  url: string;
  version: string;
  locked: boolean;
}

export interface Hello {
  sori: 1;
  id: string;
  name: string;
  version: string;
  locked: boolean;
  pin?: boolean;
}

/** 이 화면을 이 기기 안(앱, 또는 같은 컴퓨터의 브라우저)에서 열었는가. */
export function isLocalOrigin(): boolean {
  const h = location.hostname;
  return h === '127.0.0.1' || h === 'localhost' || h === '[::1]' || h === '::1';
}

const VIA = 'sori.via';
/** 돌아갈 곳은 이 기기 안의 소리뿐이다(바깥 주소로 튕기지 않게). */
const HOME = /^http:\/\/(127\.0\.0\.1|localhost)(:\d{1,5})?\//;

/** `?via=`로 들어왔으면 기억하고 주소창에서 지운다. 화면을 그리기 전에 한 번. */
export function rememberVia() {
  try {
    const sp = new URLSearchParams(location.search);
    const via = sp.get('via');
    if (!via) return;
    if (HOME.test(via)) sessionStorage.setItem(VIA, via);
    sp.delete('via');
    const q = sp.toString();
    history.replaceState(history.state, '', `${location.pathname}${q ? `?${q}` : ''}${location.hash}`);
  } catch {
    // 저장소를 못 쓰면 돌아가기 단추만 없다
  }
}

export function viaHome(): string | null {
  try {
    const v = sessionStorage.getItem(VIA);
    return v && HOME.test(v) && !isLocalOrigin() ? v : null;
  } catch {
    return null;
  }
}

export function forgetVia() {
  try {
    sessionStorage.removeItem(VIA);
  } catch {
    // 괜찮다
  }
}

/** 다른 소리로 화면을 옮긴다. 거기서 이 기기로 돌아올 수 있게 지금 주소를 넘긴다. */
export function openRemote(url: string) {
  const back = `${location.origin}/settings/connect`;
  location.href = `${url.replace(/\/+$/, '')}/?via=${encodeURIComponent(back)}`;
}

/** 사람이 넣은 주소를 다듬는다: `192.168.0.5:4390` → `http://192.168.0.5:4390` */
export function normalizeUrl(raw: string): string | null {
  const s = raw.trim().replace(/\/+$/, '');
  if (!s || s.length > 200 || /\s/.test(s)) return null;
  const url = /^https?:\/\//i.test(s) ? s : `http://${s}`;
  try {
    const u = new URL(url);
    if (u.username || u.password || !u.hostname) return null;
    return `${u.protocol}//${u.host}${u.pathname === '/' ? '' : u.pathname}`;
  } catch {
    return null;
  }
}

/** 그 주소에 소리가 있는지 묻는다(로그인 없이, 다른 출처에서도 답한다). */
export async function hello(url: string, timeoutMs = 4000): Promise<Hello> {
  const res = await fetch(`${url}/api/hello`, { cache: 'no-store', signal: AbortSignal.timeout(timeoutMs) });
  if (!res.ok) throw new Error(`${res.status}`);
  const j = (await res.json()) as Hello;
  if (j.sori !== 1) throw new Error('소리가 아닙니다');
  return j;
}

export const link = {
  info: () => fetchJson<LinkInfo>('/api/link'),
  setLan: (enabled: boolean) =>
    fetchJson<LinkInfo>('/api/link/lan', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ enabled }) }),
  renewPin: () => fetchJson<LinkInfo>('/api/link/pin', { method: 'POST' }),
  rename: (name: string) =>
    fetchJson<LinkInfo>('/api/link/name', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ name }) }),
  discover: () => fetchJson<Found[]>('/api/link/discover'),
  save: (s: Saved) =>
    fetchJson<LinkInfo>('/api/link/saved', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(s) }),
  forget: (url: string) => fetchJson<LinkInfo>(`/api/link/saved?url=${encodeURIComponent(url)}`, { method: 'DELETE' }),
};

/** 장치 존의 이름: 이 기기 안에서 연 화면이면 "이 기기", 다른 기기에서 열었으면 그 기기의 이름. */
export function nativeZoneName(deviceName?: string): string {
  return isLocalOrigin() ? '이 기기' : (deviceName ?? '그 기기');
}
