#!/usr/bin/env python3
"""장비 손질 창 미리보기: 창 제목(JSON)을 바닐라처럼 그려 봅니다 (진짜 클라이언트 없이 자리 확인용).

    FORGE_PREVIEW=<폴더> cargo test --release --test forge
    cargo run --release --example resource_pack -- <폴더>/pack.zip
    python3 tools/forge_preview.py <폴더> [docs/forge_preview.png]

GUI 배율 2, 화면 480×270(GUI px)으로 그립니다. 글자는 Unifont(시스템 글꼴)로 그려 바닐라와 모양은
조금 다르지만 폭·자리는 서버가 쓰는 표와 같습니다. 아이템은 미리보기 이름마다 정해 둔 그림을 놓습니다.
"""
import json, re, sys, zipfile, io, os
from PIL import Image, ImageDraw, ImageFont

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
SCALE = 2
SCREEN = (480, 270)
CHEST = (176, 222)
TITLE = (8, 6)
UNIFONT = "/usr/share/fonts/opentype/unifont/unifont.otf"

def rust_array(path, name):
    src = open(os.path.join(ROOT, path), encoding="utf-8").read()
    m = re.search(name + r"[^=]*=\s*\[(.*?)\];", src, re.S)
    return [int(x, 0) for x in re.findall(r"0x[0-9A-Fa-f]+|\d+", m.group(1))]

ASCII_ADVANCE = rust_array("src/plugins/hud/glyphs.rs", "pub const ASCII_ADVANCE")
PALETTE = rust_array("src/plugins/hud/art.rs", "pub const PALETTE")

def advance_default(c):
    o = ord(c)
    if o < 128:
        return ASCII_ADVANCE[o] or None
    if 0xAC00 <= o <= 0xD7A3:
        return 8
    if 0x1100 <= o <= 0x11FF or 0x3001 <= o <= 0x30FF or 0x3130 <= o <= 0x318F or 0x3200 <= o <= 0x9FFF:
        return 9
    return None

def space_advance(c):
    o = ord(c)
    if 0xE400 <= o <= 0xE40A:
        return 1 << (o - 0xE400)
    if 0xE410 <= o <= 0xE41A:
        return -(1 << (o - 0xE410))
    return None

# 슬롯 아이템 (미리보기 이름 → [(칸, 그림)])
HELM = "assets/hoya/textures/item/equip/mithril_helmet.png"
RING = "assets/hoya/textures/item/equip/mithril_ring.png"
SCROLL = "assets/hoya/textures/item/equip/scroll_low.png"
STONE = "assets/hoya/textures/item/equip/enhance_stone.png"
ITEMS = {
    "enhance": [(22, HELM)],
    "enhance_artisan": [(22, HELM)],
    "inherit": [(20, HELM), (24, HELM)],
    "polish": [(22, RING), (25, SCROLL)],
    "tune": [(22, RING)],
    "salvage": [(22, RING)],
}
BAG = [(0, HELM), (1, HELM), (2, STONE), (3, RING), (4, SCROLL)]

def load_pack(zpath):
    z = zipfile.ZipFile(zpath)
    font = json.loads(z.read("assets/hoya/font/gui/forge.json"))
    glyphs = {}
    for p in font["providers"]:
        if p["type"] != "bitmap":
            continue
        ns, name = p["file"].split(":")
        img = Image.open(io.BytesIO(z.read(f"assets/{ns}/textures/{name}"))).convert("RGBA")
        glyphs[p["chars"][0]] = (img, p["ascent"], p["height"])
    def tex(path):
        try:
            return Image.open(io.BytesIO(z.read(path))).convert("RGBA")
        except KeyError:
            return None
    return glyphs, tex

def ink_width(img):
    a = img.getchannel("A")
    w, h = img.size
    for x in range(w - 1, -1, -1):
        if any(a.getpixel((x, y)) for y in range(h)):
            return x + 1
    return 0

def tinted(img, rgb):
    if rgb == (255, 255, 255):
        return img
    r, g, b, a = img.split()
    r = r.point(lambda v: v * rgb[0] // 255)
    g = g.point(lambda v: v * rgb[1] // 255)
    b = b.point(lambda v: v * rgb[2] // 255)
    return Image.merge("RGBA", (r, g, b, a))

def runs(tag):
    out = []
    def walk(t):
        if t.get("text"):
            out.append((t.get("font", "minecraft:default"), t.get("color"), t["text"]))
        for e in t.get("extra", []):
            walk(e)
    walk(tag)
    return out

def render(title, glyphs, tex, items):
    W, H = SCREEN[0] * SCALE, SCREEN[1] * SCALE
    im = Image.new("RGBA", (W, H), (0, 0, 0, 255))
    # 게임 화면 + 바닐라가 깔아 주는 어두운 막
    d = ImageDraw.Draw(im)
    for y in range(H):
        t = y / H
        d.line([(0, y), (W, y)], fill=(int(70 + 40 * t), int(110 + 30 * t), int(150 - 60 * t), 255))
    im.alpha_composite(Image.new("RGBA", (W, H), (16, 16, 16, 190)))
    left = (SCREEN[0] - CHEST[0]) // 2
    top = (SCREEN[1] - CHEST[1]) // 2
    # 바닐라 상자 바탕 (회색) — 우리 그림이 덮어야 함
    d.rectangle([left * SCALE, top * SCALE, (left + CHEST[0]) * SCALE - 1, (top + CHEST[1]) * SCALE - 1], fill=(198, 198, 198, 255))
    font = ImageFont.truetype(UNIFONT, 16)
    x = left + TITLE[0]
    ty = top + TITLE[1]
    for fid, color, text in runs(title):
        dy, rgb = 0, (255, 255, 255)
        if color and color.upper().startswith("#FA"):
            v = int(color[3:], 16)
            dy = (v >> 8) - 32
            rgb = tuple((PALETTE[(v & 0xff) - 0xF0] >> s) & 0xff for s in (16, 8, 0))
        for c in text:
            sa = space_advance(c)
            if sa is not None:
                x += sa
                continue
            if fid == "hoya:gui/forge":
                img, ascent, height = glyphs[c]
                f = height / img.size[1]
                gy = ty + 7 - ascent + dy
                scaled = img.resize((round(img.size[0] * f * SCALE), round(height * SCALE)), Image.NEAREST)
                im.alpha_composite(tinted(scaled, rgb), (x * SCALE, gy * SCALE))
                x += int(0.5 + ink_width(img) * f) + 1
            else:
                a = advance_default(c)
                if a is None:
                    continue
                gy = ty + dy
                d = ImageDraw.Draw(im)
                d.text((x * SCALE, gy * SCALE - 1), c, font=font, fill=rgb + (255,))
                x += a
    # 바닐라 "인벤토리" 글 (회색 404040)
    d = ImageDraw.Draw(im)
    d.text(((left + 8) * SCALE, (top + 128) * SCALE - 1), "인벤토리", font=font, fill=(64, 64, 64, 255))
    # 아이템
    def put(slot_xy, path):
        t = tex(path)
        if t is None:
            return
        t = t.resize((16 * SCALE, 16 * SCALE), Image.NEAREST)
        im.alpha_composite(t, ((left + slot_xy[0]) * SCALE, (top + slot_xy[1]) * SCALE))
    for slot, path in items:
        put((8 + 18 * (slot % 9), 18 + 18 * (slot // 9)), path)
    for i, path in BAG:
        put((8 + 18 * i, 197), path)
    return im

def main():
    folder = sys.argv[1]
    out = sys.argv[2] if len(sys.argv) > 2 else os.path.join(folder, "forge_preview.png")
    glyphs, tex = load_pack(os.path.join(folder, "pack.zip"))
    names = ["empty", "enhance", "enhance_artisan", "inherit", "polish", "tune", "salvage"]
    shots = []
    for n in names:
        p = os.path.join(folder, n + ".json")
        if not os.path.exists(p):
            continue
        im = render(json.load(open(p, encoding="utf-8")), glyphs, tex, ITEMS.get(n, []))
        im.save(os.path.join(folder, n + ".png"))
        shots.append(im)
    # 모아 보기 (두 줄)
    cols = 2
    w, h = shots[0].size
    sheet = Image.new("RGBA", (w * cols, h * ((len(shots) + cols - 1) // cols)), (0, 0, 0, 255))
    for i, s in enumerate(shots):
        sheet.paste(s, ((i % cols) * w, (i // cols) * h))
    sheet.convert("RGB").save(out)
    print(out)

main()
