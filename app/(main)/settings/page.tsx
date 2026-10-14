import type { Metadata } from 'next';
import { GeneralPrefs } from '@/components/prefs-client';

export const metadata: Metadata = { title: '설정 · 일반' };

export default function GeneralSettings() {
  return (
    <section>
      <h2 className="settings-h2">일반</h2>
      <p className="settings-lede">이 기기에서의 화면 취향입니다. 브라우저에 저장되며 다른 기기에는 영향을 주지 않습니다.</p>
      <GeneralPrefs />
    </section>
  );
}
