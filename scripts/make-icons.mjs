import fs from 'node:fs';
import path from 'node:path';
import zlib from 'node:zlib';

/**
 * 앱 아이콘을 만든다. 의존성 없이, PNG를 직접 써서.
 *
 * 아이콘 두 장 때문에 이미지 라이브러리를 받아 오는 건 과하다. PNG는 압축된 픽셀
 * 배열에 청크 몇 개를 두른 형식이고, 압축은 zlib이 하므로 여기서 할 일은 픽셀을
 * 칠하는 것뿐이다.
 *
 *   node scripts/make-icons.mjs
 */

const OUT = path.join(process.cwd(), 'public');

const BG = [15, 17, 21, 255]; // 앱 배경과 같은 색
const WHITE = [247, 247, 244, 255]; // 흰 공
const YELLOW = [255, 212, 58, 255]; // 노란 공

/** 안티에일리어싱 없이 그린 원 두 개 — 아이콘 크기에서는 가장자리가 4배 샘플링으로 충분하다. */
function draw(size) {
  const pixels = Buffer.alloc(size * size * 4);

  // 두 공의 중심과 반지름. 겹쳐 놓으면 당구공 두 개로 읽힌다.
  const r = size * 0.24;
  const white = { x: size * 0.37, y: size * 0.42 };
  const yellow = { x: size * 0.63, y: size * 0.6 };

  const inside = (cx, cy, x, y) => (x - cx) ** 2 + (y - cy) ** 2 <= r * r;

  for (let y = 0; y < size; y++) {
    for (let x = 0; x < size; x++) {
      // 픽셀 하나를 네 점으로 나눠 보고 평균을 낸다. 가장자리 계단이 이걸로 사라진다.
      let rgb = [0, 0, 0, 0];
      for (const [dx, dy] of [
        [0.25, 0.25],
        [0.75, 0.25],
        [0.25, 0.75],
        [0.75, 0.75],
      ]) {
        const px = x + dx;
        const py = y + dy;
        // 노란 공이 위에 온다 — 겹치는 쪽이 하나로 정해져 있어야 형태가 또렷하다.
        const color = inside(yellow.x, yellow.y, px, py)
          ? YELLOW
          : inside(white.x, white.y, px, py)
            ? WHITE
            : BG;
        rgb = rgb.map((value, index) => value + color[index]);
      }

      const at = (y * size + x) * 4;
      for (let i = 0; i < 4; i++) pixels[at + i] = Math.round(rgb[i] / 4);
    }
  }

  return pixels;
}

/** 스캔라인마다 필터 바이트 0을 붙인 원시 데이터가 PNG의 IDAT 내용이다. */
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
  ihdr[8] = 8; // 비트 깊이
  ihdr[9] = 6; // 알파 있는 트루컬러
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
