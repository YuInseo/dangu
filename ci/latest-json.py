#!/usr/bin/env python3
"""릴리스에 함께 올릴 latest.json을 만든다. 앱의 자동 업데이트가 이것을 본다.

    BUILD_TIME=<커밋 시각(유닉스 초)> python3 ci/latest-json.py <올릴 폴더> > <올릴 폴더>/latest.json

빌드(커밋), 시각, 바뀐 것(커밋 메시지 첫 줄), 그리고 기기별 파일의 주소·크기·SHA-256.
"""
import glob
import hashlib
import json
import os
import sys

up = sys.argv[1]
repo = os.environ.get('GITHUB_REPOSITORY', 'YuInseo/dangu')
base = f'https://github.com/{repo}/releases/download/sori-latest'


def asset(pattern):
    files = sorted(glob.glob(os.path.join(up, pattern)))
    if not files:
        return None
    path = files[0]
    h = hashlib.sha256()
    with open(path, 'rb') as f:
        for chunk in iter(lambda: f.read(1 << 20), b''):
            h.update(chunk)
    return {'url': f'{base}/{os.path.basename(path)}', 'size': os.path.getsize(path), 'sha256': h.hexdigest()}


print(json.dumps({
    'build': os.environ['GITHUB_SHA'],
    'time': int(os.environ['BUILD_TIME']),
    'notes': os.environ.get('NOTES', '').strip().split('\n')[0][:200],
    'android': asset('Sori-android.apk'),
    'windows': asset('*-setup.exe'),
}, ensure_ascii=False, indent=1))
