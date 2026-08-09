import { SettingsPanel } from './settings-panel';
import { TopBar } from '../topbar';

export const metadata = { title: '설정' };

export default function SettingsPage() {
  return (
    <div className="app">
      <TopBar title="설정" back="/" />
      <SettingsPanel />
    </div>
  );
}
