import { useTitle } from '@/api';
import { DspPage } from '@/components/dsp/dsp-page';

export function DspScreen() {
  useTitle('소리결 DSP');
  return <DspPage />;
}
