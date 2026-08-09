import { Lobby } from './lobby';
import { TopBar } from './topbar';

export const metadata = { title: '당구 점수판' };

/**
 * 첫 화면.
 *
 * 서버 컴포넌트지만 하는 일은 껍데기를 그리는 것뿐이다. 점수판의 상태는 전부 기기에
 * 있으므로 서버가 알 것도, 기다릴 것도 없다 — 그래서 이 페이지는 정적이고, 오프라인
 * 캐시에 그대로 들어간다.
 */
export default function HomePage() {
  return (
    <div className="app">
      <TopBar title="당구 점수판" />
      <Lobby />
    </div>
  );
}
