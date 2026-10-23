#!/usr/bin/env python3
"""툴팁(아이템 설명) 미리보기: `cargo test lore_showcase`가 쓴 JSON을 마인크래프트 툴팁처럼 그립니다.

    MC_LORE_PREVIEW_JSON=/tmp/lore.json cargo test --lib lore_showcase -- --ignored
    python3 tools/lore_preview.py /tmp/lore.json <ascii.png> <unifont.hex> docs/lore_preview.png

ascii.png는 바닐라 assets/minecraft/textures/font/ascii.png, unifont.hex는 GNU Unifont 헥스 파일.
GUI 배율 2로 그린 뒤 두 배로 키웁니다 (바닐라 글자 1픽셀 = 4칸, unifont 1픽셀 = 2칸).
"""
import json
import sys

from PIL import Image

S = 2  # GUI 배율
NAMED = {
    "black": "000000", "dark_blue": "0000AA", "dark_green": "00AA00", "dark_aqua": "00AAAA",
    "dark_red": "AA0000", "dark_purple": "AA00AA", "gold": "FFAA00", "gray": "AAAAAA",
    "dark_gray": "555555", "blue": "5555FF", "green": "55FF55", "aqua": "55FFFF", "red": "FF5555",
    "light_purple": "FF55FF", "yellow": "FFFF55", "white": "FFFFFF",
}
CODES = dict(zip("0123456789abcdef", NAMED.values()))


def rgb(c):
    c = NAMED.get(c, c).lstrip("#")
    return tuple(int(c[i:i + 2], 16) for i in (0, 2, 4))


class Font:
    def __init__(self, ascii_png, hexfile):
        self.ascii = Image.open(ascii_png).convert("RGBA")
        self.uni = {}
        with open(hexfile) as f:
            for line in f:
                cp, data = line.strip().split(":")
                self.uni[int(cp, 16)] = data

    def glyph(self, ch):
        """(픽셀 목록 [(x,y)], 화면 칸 크기, GUI 전진 폭)"""
        o = ord(ch)
        if ch == " ":
            return [], S, 4
        if o < 128 and o != 0:
            gx, gy = (o % 16) * 8, (o // 16) * 8
            px = [(x, y) for y in range(8) for x in range(8) if self.ascii.getpixel((gx + x, gy + y))[3] > 0]
            if px:
                w = max(x for x, _ in px) + 1
                return px, S, w + 1
        data = self.uni.get(o)
        if data is None:
            return [], S, 6
        w = len(data) // 16 * 4  # 8 또는 16
        rows = [int(data[i:i + w // 4], 16) for i in range(0, len(data), w // 4)]
        px = [(x, y) for y in range(16) for x in range(w) if rows[y] & (1 << (w - 1 - x))]
        right = max((x for x, _ in px), default=w - 1) + 1
        return px, S // 2, (right + 1) // 2 + 1


def runs(comp, inherit=None):
    """텍스트 컴포넌트 → (글, 색, 굵게) 조각들. § 코드도 풂."""
    style = dict(inherit or {"color": "AAAAAA", "bold": False})
    if isinstance(comp, str):
        comp = {"text": comp}
    if "color" in comp:
        style["color"] = comp["color"]
    if "bold" in comp:
        style["bold"] = bool(comp["bold"])
    out = []
    text = comp.get("text", comp.get("translate", ""))
    cur = dict(style)
    buf = ""
    i = 0
    while i < len(text):
        if text[i] == "§" and i + 1 < len(text):
            if buf:
                out.append((buf, cur["color"], cur["bold"]))
                buf = ""
            k = text[i + 1].lower()
            if k in CODES:
                cur = {"color": CODES[k], "bold": False}
            elif k == "l":
                cur["bold"] = True
            elif k == "r":
                cur = dict(style)
            i += 2
            continue
        buf += text[i]
        i += 1
    if buf:
        out.append((buf, cur["color"], cur["bold"]))
    for e in comp.get("extra", []):
        out.extend(runs(e, style))
    return out


def width(font, parts):
    return sum(font.glyph(ch)[2] + (1 if b else 0) for t, _, b in parts for ch in t)


def draw_text(img, font, parts, x, y):
    for t, color, bold in parts:
        c = rgb(color)
        shadow = tuple(v // 4 for v in c)
        for ch in t:
            px, cell, adv = font.glyph(ch)
            for dx_b in ([0, 1] if bold else [0]):
                for off, col in (((1, 1), shadow), ((0, 0), c)):
                    for gx, gy in px:
                        X = x * S + dx_b * S + off[0] * S + gx * cell
                        Y = y * S + off[1] * S + gy * cell
                        for a in range(cell):
                            for b in range(cell):
                                if 0 <= X + a < img.width and 0 <= Y + b < img.height:
                                    img.putpixel((X + a, Y + b), col + (255,))
            x += adv + (1 if bold else 0)


def tooltip(font, tip):
    lines = [runs(tip["name"])] + [runs(l) for l in tip["lore"]]
    w = max(width(font, l) for l in lines) + 8
    h = 10 * len(lines) + 2 + 6
    img = Image.new("RGBA", (w * S, h * S), (0, 0, 0, 0))
    bg = (16, 0, 16, 240)
    for yy in range(h * S):
        for xx in range(w * S):
            img.putpixel((xx, yy), bg)
    # 테두리: 위 보라 → 아래 짙은 보라
    for yy in range(1 * S, (h - 1) * S):
        t = yy / (h * S)
        col = (int(0x50 * (1 - t) + 0x28 * t), 0, int(0xFF * (1 - t) + 0x7F * t), 255)
        for xx in list(range(1 * S, 2 * S)) + list(range((w - 2) * S, (w - 1) * S)):
            img.putpixel((xx, yy), col)
    for xx in range(1 * S, (w - 1) * S):
        for yy in range(1 * S, 2 * S):
            img.putpixel((xx, yy), (0x50, 0, 0xFF, 255))
        for yy in range((h - 2) * S, (h - 1) * S):
            img.putpixel((xx, yy), (0x28, 0, 0x7F, 255))
    y = 4
    for i, l in enumerate(lines):
        draw_text(img, font, l, 4, y)
        y += 10 + (2 if i == 0 else 0)
    return img


def main():
    tips_path, ascii_png, hexfile, out = sys.argv[1:5]
    font = Font(ascii_png, hexfile)
    tips = [tooltip(font, t) for t in json.load(open(tips_path))]
    cols = 3
    rows = [tips[i:i + cols] for i in range(0, len(tips), cols)]
    pad = 12 * S
    W = max(sum(t.width for t in r) + pad * (len(r) + 1) for r in rows)
    H = sum(max(t.height for t in r) for r in rows) + pad * (len(rows) + 1)
    sheet = Image.new("RGBA", (W, H), (0, 0, 0, 255))
    for yy in range(H):
        v = 40 + yy * 30 // H
        for xx in range(W):
            sheet.putpixel((xx, yy), (v // 2, v // 2 + 6, v, 255))
    y = pad
    for r in rows:
        x = pad
        for t in r:
            sheet.alpha_composite(t, (x, y))
            x += t.width + pad
        y += max(t.height for t in r) + pad
    sheet = sheet.resize((W * 2, H * 2), Image.NEAREST)
    sheet.convert("RGB").save(out)


if __name__ == "__main__":
    main()
