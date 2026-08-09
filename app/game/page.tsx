import { Scoreboard } from './scoreboard';
import { TopBar } from '../topbar';

export const metadata = { title: '점수판' };

/**
 * 점수판 화면.
 *
 * 상단 줄은 로비와 같은 것을 쓴다 — 게임 중에도 설정과 업데이트에 닿을 수 있어야
 * 하고, 위치가 화면마다 바뀌면 그게 더 헷갈린다.
 */
export default function GamePage() {
  return (
    <div className="app">
      <TopBar title="점수판" back="/" />
      <Scoreboard />
    </div>
  );
}
