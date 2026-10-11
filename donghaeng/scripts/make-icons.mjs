import fs from 'node:fs';
import path from 'node:path';
import zlib from 'node:zlib';

/**
 * 앱 아이콘을 만든다. 의존성 없이, PNG를 직접 써서 (당구 점수판의 것과 같은 방식).
 *
 * 남색 바탕에 흰 쪽지 한 장. 쪽지 위의 두 줄이 출발(파랑)과 도착(빨강)이다.
 *
 *   node scripts/make-icons.mjs
 */

const OUT = path.join(path.dirname(new URL(import.meta.url).pathname), '..', 'public');

const NAVY = [29, 58, 110, 255];
const PAPER = [255, 255, 255, 255];
const BLUE = [29, 95, 209, 255];
const RED = [207, 42, 42, 255];
const INK = [190, 198, 212, 255];

/** 모서리가 둥근 사각형 안인지. 좌표는 0–1 비율. */
function inRoundRect(x, y, left, top, right, bottom, radius) {
  if (x < left || x > right || y < top || y > bottom) return false;
  const cx = Math.min(Math.max(x, left + radius), right - radius);
  const cy = Math.min(Math.max(y, top + radius), bottom - radius);
  return (x - cx) ** 2 + (y - cy) ** 2 <= radius ** 2;
}

function colorAt(x, y) {
  // 마스커블 아이콘의 안전 영역(가운데 80%) 안에 다 들어가게 그린다.
  if (inRoundRect(x, y, 0.24, 0.26, 0.76, 0.74, 0.04)) {
    if (inRoundRect(x, y, 0.31, 0.35, 0.69, 0.42, 0.035)) return BLUE;
    if (inRoundRect(x, y, 0.31, 0.47, 0.69, 0.54, 0.035)) return RED;
    if (inRoundRect(x, y, 0.31, 0.59, 0.55, 0.645, 0.025)) return INK;
    return PAPER;
  }
  return NAVY;
}

function draw(size) {
  const pixels = Buffer.alloc(size * size * 4);
  const samples = [0.25, 0.75];
  for (let y = 0; y < size; y++) {
    for (let x = 0; x < size; x++) {
      const sum = [0, 0, 0, 0];
      for (const dy of samples) {
        for (const dx of samples) {
          const color = colorAt((x + dx) / size, (y + dy) / size);
          for (let i = 0; i < 4; i++) sum[i] += color[i];
        }
      }
      const at = (y * size + x) * 4;
      for (let i = 0; i < 4; i++) pixels[at + i] = Math.round(sum[i] / 4);
    }
  }
  return pixels;
}

function encode(size, pixels) {
  const stride = size * 4;
  const raw = Buffer.alloc((stride + 1) * size);
  for (let y = 0; y < size; y++) {
    raw[y * (stride + 1)] = 0;
    pixels.copy(raw, y * (stride + 1) + 1, y * stride, (y + 1) * stride);
  }
  const chunk = (type, data) => {
    const out = Buffer.alloc(8 + data.length + 4);
    out.writeUInt32BE(data.length, 0);
    out.write(type, 4);
    data.copy(out, 8);
    out.writeUInt32BE(zlib.crc32(Buffer.concat([Buffer.from(type), data])), 8 + data.length);
    return out;
  };
  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(size, 0);
  ihdr.writeUInt32BE(size, 4);
  ihdr[8] = 8;
  ihdr[9] = 6;
  return Buffer.concat([
    Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
    chunk('IHDR', ihdr),
    chunk('IDAT', zlib.deflateSync(raw, { level: 9 })),
    chunk('IEND', Buffer.alloc(0)),
  ]);
}

fs.mkdirSync(OUT, { recursive: true });
for (const size of [192, 512]) {
  const file = path.join(OUT, `icon-${size}.png`);
  fs.writeFileSync(file, encode(size, draw(size)));
  console.log(`  ${path.relative(process.cwd(), file)}`);
}
