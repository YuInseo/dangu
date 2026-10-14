import type { Metadata } from 'next';
import { DspPage } from '@/components/dsp/dsp-page';

export const metadata: Metadata = { title: '소리결 DSP' };

export default function Page() {
  return <DspPage />;
}
