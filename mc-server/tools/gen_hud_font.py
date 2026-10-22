#!/usr/bin/env python3
"""HUD 글자 자료를 만듭니다 → src/plugins/hud/glyphs.rs

- 바닐라 기본 폰트(ascii.png)의 글자 폭: HUD가 기본 폰트 글(퀘스트 이름·닉네임)의 폭을 재서 줄을
  맞추는 데 씁니다. 한글 음절은 바닐라 unifont 설정(size_overrides left 1, right 15)으로 8 고정.
- 라벨용 한글 글리프 몇 개: GNU Unifont (SIL Open Font License 1.1)에서 뽑아 HUD 폰트에 넣습니다.

    python3 tools/gen_hud_font.py <mcmeta 26.3-assets>/assets/minecraft <unifont .hex> > src/plugins/hud/glyphs.rs

<assets>에는 font/include/default.json과 textures/font/ascii.png만 있으면 됩니다.
"""
import sys, json

LABELS = "돈캐시잠수포인트퀘스트레벨공격력방어력체력마나속도전투력주간일반길드완료진행중없음오늘이번주"
# 미리보기(테스트)에서 바닐라 글꼴로 그릴 한글 (HUD 미리보기 예시 글에 나오는 것)
PREVIEW = "나무베기철광석캐농부의탁퀘스트로받기주간길드일반완료개진행중"

def read_png(path):
    from PIL import Image  # pip install pillow
    img = Image.open(path).convert('RGBA')
    w, h = img.size
    px = img.load()
    return w, h, lambda x, y: px[x, y][3]

def main():
    assets, hexfile = sys.argv[1], sys.argv[2]
    default = json.load(open(f'{assets}/font/include/default.json'))
    ascii_p = next(p for p in default['providers'] if p.get('file') == 'minecraft:font/ascii.png')
    w, h, alpha = read_png(f'{assets}/textures/font/ascii.png')
    cw, ch = w // 16, h // len(ascii_p['chars'])
    adv = [0] * 128
    for row, chars in enumerate(ascii_p['chars']):
        for col, c in enumerate(chars):
            o = ord(c)
            if o >= 128 or c == '\0':
                continue
            ink = 0
            for x in range(cw - 1, -1, -1):
                if any(alpha(col*cw + x, row*ch + y) for y in range(ch)):
                    ink = x + 1; break
            adv[o] = ink + 1
    adv[32] = 4
    # 글자 그림 (미리보기용): 줄마다 8비트, 왼쪽이 최상위 비트
    bitmaps = [[0] * ch for _ in range(128)]
    for row, chars in enumerate(ascii_p['chars']):
        for col, c in enumerate(chars):
            o = ord(c)
            if o >= 128 or c == '\0':
                continue
            for y in range(ch):
                bits = 0
                for x in range(cw):
                    if alpha(col*cw + x, row*ch + y):
                        bits |= 0x80 >> x
                bitmaps[o][y] = bits
    glyphs = {}
    want = {f'{ord(c):04X}' for c in set(LABELS + PREVIEW)}
    for line in open(hexfile):
        cp, _, bits = line.strip().partition(':')
        if cp in want:
            glyphs[chr(int(cp, 16))] = bits
    print('//! tools/gen_hud_font.py가 만든 파일 (고치지 마세요).')
    print('//! 라벨 글리프: GNU Unifont 17.0.01 (SIL Open Font License 1.1).')
    print()
    print('/// 바닐라 기본 폰트(ascii.png)의 글자 폭 (0이면 그 글자 없음).')
    print('pub const ASCII_ADVANCE: [u8; 128] = [' + ', '.join(map(str, adv)) + '];')
    print()
    print()
    print('/// 바닐라 ascii.png 글자 그림 8×8 (줄마다 왼쪽이 최상위 비트). HUD 미리보기용.')
    print('#[cfg(test)]')
    print('pub const ASCII_GLYPHS: [[u8; 8]; 128] = [')
    for b in bitmaps:
        print('    [' + ', '.join(f'0x{v:02x}' for v in b) + '],')
    print('];')
    def table(name, doc, chars, test=False):
        print()
        print(doc)
        if test:
            print('#[cfg(test)]')
        print(f'pub const {name}: &[(char, [u16; 16])] = &[')
        for c in sorted(set(chars)):
            b = glyphs[c]
            assert len(b) == 64, c
            rows = [int(b[i*4:i*4+4], 16) for i in range(16)]
            print(f"    ('{c}', [{', '.join(f'0x{r:04x}' for r in rows)}]),")
        print('];')
    table('LABEL_GLYPHS', '/// 라벨용 한글 16×16 글리프 (줄마다 왼쪽이 최상위 비트).', LABELS)
    table('PREVIEW_GLYPHS', '/// 미리보기에서 바닐라 unifont 대신 그릴 한글 16×16.', PREVIEW, test=True)

main()
