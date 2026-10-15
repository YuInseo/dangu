/**
 * 지금 소리 낼 곳(존)의 이름과 출력. DSP 화면·신호 경로가 "어디로 가는지"를 말할 때 쓴다.
 */
import { nativeZoneName } from '@/lib/link';
import { useLibraryInfo } from '@/library';
import { useEngineState, usePlayer, useQueue, useZone } from './context';
import { NativeEngine } from './native';

export function useZoneOutput() {
  const { zone } = useZone();
  const { engine } = usePlayer();
  const { outputRate } = useQueue();
  // 장치 정보는 엔진 상태와 함께 바뀐다
  useEngineState();
  const deviceName = useLibraryInfo().deviceName;
  const native = zone === 'native' && engine instanceof NativeEngine ? engine : null;
  return {
    kind: native ? ('native' as const) : ('browser' as const),
    name: native ? nativeZoneName(deviceName) : '이 브라우저',
    rate: native ? (native.output?.rate ?? null) : outputRate,
    device: native?.output ? `${native.output.device} · ${native.output.api}` : null,
  };
}
