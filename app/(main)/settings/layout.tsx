import type { ReactNode } from 'react';
import { SettingsNav } from '@/components/settings-nav';

export default function SettingsLayout({ children }: { children: ReactNode }) {
  return (
    <div className="settings">
      <SettingsNav />
      <div className="settings-body">{children}</div>
    </div>
  );
}
