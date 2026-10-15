/** 초 → "3:07", "1:02:45". */
export function clock(sec: number): string {
  if (!Number.isFinite(sec) || sec < 0) sec = 0;
  const s = Math.floor(sec % 60);
  const m = Math.floor((sec / 60) % 60);
  const h = Math.floor(sec / 3600);
  const ss = String(s).padStart(2, '0');
  return h > 0 ? `${h}:${String(m).padStart(2, '0')}:${ss}` : `${m}:${ss}`;
}

/** 초 → "48분", "2시간 13분". 앨범 길이처럼 대략이면 되는 곳. */
export function span(sec: number): string {
  const totalMin = Math.round(sec / 60);
  if (totalMin < 60) return `${Math.max(totalMin, 1)}분`;
  const h = Math.floor(totalMin / 60);
  const m = totalMin % 60;
  if (h >= 24) {
    const d = Math.floor(h / 24);
    return `${d}일 ${h % 24}시간`;
  }
  return m ? `${h}시간 ${m}분` : `${h}시간`;
}

export function bytes(n: number): string {
  const units = ['B', 'KB', 'MB', 'GB', 'TB'];
  let i = 0;
  while (n >= 1024 && i < units.length - 1) {
    n /= 1024;
    i++;
  }
  return `${n >= 100 || i === 0 ? Math.round(n) : n.toFixed(1)} ${units[i]}`;
}

export function count(n: number): string {
  return n.toLocaleString('ko-KR');
}

/** 지난 때 → "방금", "5분 전", "3시간 전", "어제", "4일 전", "2025. 3. 1." */
export function ago(ms: number, now = Date.now()): string {
  const s = Math.max(0, (now - ms) / 1000);
  if (s < 60) return '방금';
  if (s < 3600) return `${Math.floor(s / 60)}분 전`;
  if (s < 86400) return `${Math.floor(s / 3600)}시간 전`;
  const d = Math.floor(s / 86400);
  if (d === 1) return '어제';
  if (d < 30) return `${d}일 전`;
  return new Date(ms).toLocaleDateString('ko-KR');
}
