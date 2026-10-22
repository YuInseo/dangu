#!/usr/bin/env python3
"""바닐라 아이템 모양 정의(assets/minecraft/items/*.json)를 한 파일로 묶어 gzip합니다.

입력: misode/mcmeta `26.3-summary` 브랜치의 assets/item_definition/data.min.json
사용: python3 -I tools/gen_item_definitions.py <data.min.json> src/data/item_definitions.json.gz
"""

import gzip
import shutil
import sys

with open(sys.argv[1], "rb") as f, gzip.GzipFile(sys.argv[2], "wb", compresslevel=9, mtime=0) as g:
    shutil.copyfileobj(f, g)
