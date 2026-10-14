import type { Metadata } from 'next';
import { config } from '@/lib/config';

export const metadata: Metadata = { title: '설정 · 정보' };

export default function AboutSettings() {
  return (
    <section>
      <h2 className="settings-h2">정보</h2>
      <div className="pref-row">
        <div className="pref-label">버전</div>
        <div className="pref-value">Sori {config.version}</div>
      </div>
      <div className="pref-row">
        <div className="pref-label">단축키</div>
        <div className="pref-value keys">
          <span>
            <kbd>Space</kbd> 재생/일시정지
          </span>
          <span>
            <kbd>←</kbd>
            <kbd>→</kbd> 10초
          </span>
          <span>
            <kbd>Shift</kbd>+<kbd>←</kbd>
            <kbd>→</kbd> 이전/다음 곡
          </span>
          <span>
            <kbd>M</kbd> 음소거
          </span>
          <span>
            <kbd>/</kbd> 검색
          </span>
          <span>
            <kbd>Q</kbd> 대기열
          </span>
          <span>
            <kbd>F</kbd> 지금 재생 중
          </span>
        </div>
      </div>
      <div className="pref-row">
        <div className="pref-label">라이브 아카이브</div>
        <div className="pref-value">
          공연 음원과 정보는{' '}
          <a href="https://archive.org/details/etree" target="_blank" rel="noreferrer">
            Internet Archive Live Music Archive
          </a>
          에서 옵니다.
        </div>
      </div>
      {config.password && (
        <div className="pref-row">
          <div className="pref-label">로그인</div>
          <form action="/api/logout" method="post">
            <button type="submit" className="btn btn-sm">
              이 기기에서 로그아웃
            </button>
          </form>
        </div>
      )}
    </section>
  );
}
