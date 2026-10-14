/**
 * FLAC 파일 머리의 STREAMINFO만 읽는다.
 *
 * 라이브 아카이브는 "Flac"이라고만 알려 주고 샘플레이트·비트 깊이는 말하지 않는다.
 * 파일 앞 몇 KB만 받아 여기서 읽으면 신호 경로에 정확한 값을 적을 수 있다.
 */
export interface StreamInfo {
  sampleRate: number;
  channels: number;
  bitDepth: number;
  totalSamples: number;
}

export function parseFlacStreamInfo(buf: Uint8Array): StreamInfo | null {
  let off = 0;
  // 앞에 ID3v2 태그가 붙은 FLAC도 있다.
  if (buf[0] === 0x49 && buf[1] === 0x44 && buf[2] === 0x33 && buf.length >= 10) {
    const size = ((buf[6]! & 0x7f) << 21) | ((buf[7]! & 0x7f) << 14) | ((buf[8]! & 0x7f) << 7) | (buf[9]! & 0x7f);
    off = 10 + size + (buf[5]! & 0x10 ? 10 : 0);
  }
  if (buf.length < off + 4 + 4 + 18) return null;
  if (buf[off] !== 0x66 || buf[off + 1] !== 0x4c || buf[off + 2] !== 0x61 || buf[off + 3] !== 0x43) return null; // "fLaC"
  off += 4;
  if ((buf[off]! & 0x7f) !== 0) return null; // 첫 블록은 반드시 STREAMINFO
  off += 4;
  const b = buf.subarray(off + 10, off + 18);
  const sampleRate = (b[0]! << 12) | (b[1]! << 4) | (b[2]! >> 4);
  const channels = ((b[2]! >> 1) & 0x7) + 1;
  const bitDepth = (((b[2]! & 1) << 4) | (b[3]! >> 4)) + 1;
  const totalSamples = (b[3]! & 0x0f) * 2 ** 32 + b[4]! * 2 ** 24 + (b[5]! << 16) + (b[6]! << 8) + b[7]!;
  if (!sampleRate) return null;
  return { sampleRate, channels, bitDepth, totalSamples };
}
