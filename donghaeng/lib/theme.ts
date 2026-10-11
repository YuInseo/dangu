/**
 * 밝게·어둡게·시스템.
 *
 * 기기마다 따로 고르는 값이라 팀과 나누지 않고 이 폰의 `localStorage`에만 둔다. 앱이 뜨기
 * 전에 읽어야 첫 화면이 번쩍이지 않으므로 동기로 읽는 쪽을 쓴다(`app/layout.tsx`의 짧은
 * 스크립트가 같은 열쇠를 본다).
 */

export type ThemeChoice = 'system' | 'light' | 'dark';

export const THEME_KEY = 'donghaeng.theme';

export function loadTheme(): ThemeChoice {
  try {
    const value = localStorage.getItem(THEME_KEY);
    return value === 'light' || value === 'dark' ? value : 'system';
  } catch {
    return 'system';
  }
}

export function applyTheme(choice: ThemeChoice): void {
  if (typeof document === 'undefined') return;
  const root = document.documentElement;
  if (choice === 'system') root.removeAttribute('data-theme');
  else root.setAttribute('data-theme', choice);
}

export function saveTheme(choice: ThemeChoice): void {
  try {
    if (choice === 'system') localStorage.removeItem(THEME_KEY);
    else localStorage.setItem(THEME_KEY, choice);
  } catch {
    // 저장이 막힌 환경. 이번 실행 동안만 적용된다.
  }
  applyTheme(choice);
}
