/** 소리. 동그라미 안의 세 막대 — 소리가 나고 있다는 표시. */
export function LogoMark({ size = 28 }: { size?: number }) {
  return (
    <svg width={size} height={size} viewBox="0 0 32 32" aria-hidden>
      <defs>
        <linearGradient id="sori-g" x1="0" y1="0" x2="1" y2="1">
          <stop offset="0" stopColor="#c4b5fd" />
          <stop offset="1" stopColor="#7c6cf0" />
        </linearGradient>
      </defs>
      <circle cx="16" cy="16" r="15" fill="url(#sori-g)" />
      <rect x="9" y="12" width="3" height="8" rx="1.5" fill="#0e0b1d" />
      <rect x="14.5" y="8" width="3" height="16" rx="1.5" fill="#0e0b1d" />
      <rect x="20" y="11" width="3" height="10" rx="1.5" fill="#0e0b1d" />
    </svg>
  );
}

export function Logo() {
  return (
    <span className="logo">
      <LogoMark />
      <span className="logo-word">sori</span>
    </span>
  );
}
