/**
 * 클리핑 표시기.
 *
 * ffmpeg는 재생보다 훨씬 앞서 처리하므로(브라우저가 몇십 초씩 미리 받는다), 클리핑이 "언제
 * 감지됐는지"가 아니라 "곡의 몇 초에서 났는지"를 기록한다. 브라우저는 재생 위치가 그 시각을
 * 지날 때 표시등을 켠다 — 귀에 들리는 순간과 맞는다.
 */
interface Session {
  times: number[];
  peak: number;
  at: number;
}

const g = globalThis as typeof globalThis & { __soriClips?: Map<string, Session> };
const sessions: Map<string, Session> = (g.__soriClips ??= new Map<string, Session>());

const SID = /^[A-Za-z0-9._-]{4,80}$/;

export function validSid(sid: string | null | undefined): sid is string {
  return !!sid && SID.test(sid);
}

export function recordClip(sid: string, time: number, peakDb: number) {
  let s = sessions.get(sid);
  if (!s) {
    s = { times: [], peak: -Infinity, at: Date.now() };
    sessions.set(sid, s);
  }
  // 같은 순간이 여러 프레임에 걸치면 하나로 친다
  const last = s.times.at(-1);
  if (last == null || Math.abs(time - last) > 0.05) s.times.push(Math.round(time * 1000) / 1000);
  if (s.times.length > 5000) s.times.splice(0, s.times.length - 5000);
  s.peak = Math.max(s.peak, peakDb);
  s.at = Date.now();
  if (sessions.size > 200) {
    const cutoff = Date.now() - 30 * 60_000;
    for (const [k, v] of sessions) if (v.at < cutoff) sessions.delete(k);
  }
}

/** 탐색해서 새 스트림이 시작되면 그 위치 뒤의 기록은 다시 쌓인다. */
export function resetClipsAfter(sid: string, from: number) {
  const s = sessions.get(sid);
  if (s) s.times = s.times.filter((t) => t < from);
}

export function getClips(sid: string): { times: number[]; peak: number | null } {
  const s = sessions.get(sid);
  return { times: s?.times ?? [], peak: s && Number.isFinite(s.peak) ? Math.round(s.peak * 100) / 100 : null };
}
