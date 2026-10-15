#!/usr/bin/env python3
"""CI 로그에 화면을 base64로 찍는다(아티팩트 저장소에 닿지 못하는 곳에서 화면을 받으려고).

    python3 ci/print-shots.py shots [--width 540]

--width를 주고 Pillow가 있으면 PNG를 그 폭의 JPEG로 줄여서 찍는다.
"""
import base64
import io
import pathlib
import sys

args = sys.argv[1:]
width = None
if '--width' in args:
    i = args.index('--width')
    width = int(args[i + 1])
    del args[i:i + 2]
d = pathlib.Path(args[0] if args else 'shots')

try:
    from PIL import Image
except ImportError:
    Image = None


def payload(f: pathlib.Path) -> tuple[str, bytes]:
    if width and Image and f.suffix.lower() == '.png':
        im = Image.open(f).convert('RGB')
        if im.width > width:
            im = im.resize((width, round(im.height * width / im.width)), Image.LANCZOS)
        buf = io.BytesIO()
        im.save(buf, 'JPEG', quality=80, optimize=True)
        return f.stem + '.jpg', buf.getvalue()
    return f.name, f.read_bytes()


for f in sorted(d.glob('*')):
    if f.suffix.lower() not in ('.png', '.jpg', '.jpeg', '.txt'):
        continue
    name, raw = payload(f)
    data = base64.b64encode(raw).decode()
    print(f'SORI-SHOT-BEGIN {name} {len(data)}')
    for i in range(0, len(data), 1000):
        print(data[i:i + 1000])
    print(f'SORI-SHOT-END {name}')
