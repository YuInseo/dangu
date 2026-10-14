import type { Metadata } from 'next';
import { PlayPrefs } from '@/components/prefs-client';

export const metadata: Metadata = { title: '설정 · 재생 동작' };

export default function PlaySettings() {
  return (
    <section>
      <h2 className="settings-h2">재생 동작</h2>
      <p className="settings-lede">누르면 무엇을 할지 정합니다.</p>
      <PlayPrefs />
    </section>
  );
}
