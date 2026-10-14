/** 같은 문자열이면 같은 숫자(FNV-1a). 표지 없는 앨범에 늘 같은 그림을 그리는 데 쓴다. */
export function hash(s: string): number {
  let h = 2166136261;
  for (let i = 0; i < s.length; i++) {
    h ^= s.charCodeAt(i);
    h = Math.imul(h, 16777619);
  }
  return h >>> 0;
}

/** 표지 없는 앨범과 공연에 쓰는 두 색과 각도. */
export function artColors(seed: string) {
  const h = hash(seed);
  const a = h % 360;
  const b = (a + 40 + ((h >>> 9) % 90)) % 360;
  return { a, b, angle: 120 + ((h >>> 17) % 120) };
}
