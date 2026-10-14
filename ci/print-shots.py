#!/usr/bin/env python3
"""CI 로그에 화면을 base64로 찍는다(아티팩트 저장소에 닿지 못하는 곳에서 화면을 받으려고).

    python3 ci/print-shots.py shots
"""
import base64
import pathlib
import sys

d = pathlib.Path(sys.argv[1] if len(sys.argv) > 1 else 'shots')
for f in sorted(d.glob('*')):
    if f.suffix.lower() not in ('.png', '.jpg', '.jpeg', '.txt'):
        continue
    data = base64.b64encode(f.read_bytes()).decode()
    print(f'SORI-SHOT-BEGIN {f.name} {len(data)}')
    for i in range(0, len(data), 1000):
        print(data[i:i + 1000])
    print(f'SORI-SHOT-END {f.name}')
