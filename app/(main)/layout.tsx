import type { ReactNode } from 'react';
import { connection } from 'next/server';
import { Shell } from '@/components/shell';
import { ToastProvider } from '@/components/toast';
import { hasFfmpeg } from '@/lib/config';
import { getLibrary, getScanStatus } from '@/lib/library/store';
import { PlayerProvider } from '@/lib/player/context';

export default async function MainLayout({ children }: { children: ReactNode }) {
  // ffmpeg 여부는 빌드한 기계가 아니라 지금 돌고 있는 서버의 것이어야 한다.
  await connection();
  const [ffmpeg, lib] = await Promise.all([hasFfmpeg(), getLibrary()]);
  return (
    <ToastProvider>
      <PlayerProvider canTranscode={ffmpeg}>
        <Shell status={getScanStatus()} summary={lib.summary}>
          {children}
        </Shell>
      </PlayerProvider>
    </ToastProvider>
  );
}
