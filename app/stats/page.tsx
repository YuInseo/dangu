import { StatsView } from './stats-view';
import { TopBar } from '../topbar';
import { NavBar } from '../navbar';

export const metadata = { title: '통계' };

export default function StatsPage() {
  return (
    <div className="app">
      <TopBar title="통계" back="/" />
      <StatsView />
      <NavBar />
    </div>
  );
}
