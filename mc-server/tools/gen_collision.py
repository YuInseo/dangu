#!/usr/bin/env python3
"""충돌 모양이 없는 블록(꽃, 횃불, 레일, 표지판 …) 목록을 만듭니다.

입력은 PrismarineJS/minecraft-data `pc/26.1/blockCollisionShapes.json`. 상태에 따라 모양이
있다가 없는 블록(눈 1층 등)은 충돌하는 쪽으로 셉니다.

사용:
  python3 -I tools/gen_collision.py <blockCollisionShapes.json> src/data/collision.rs
"""

import json
import sys


def main():
    src, out = sys.argv[1], sys.argv[2]
    d = json.load(open(src))
    shapes = d["shapes"]
    names = []
    for name, v in d["blocks"].items():
        ids = v if isinstance(v, list) else [v]
        if all(len(shapes[str(i)]) == 0 for i in ids):
            names.append(name)
    with open(out, "w") as f:
        f.write("// tools/gen_collision.py가 만든 파일입니다. 손으로 고치지 마세요.\n")
        f.write("// 충돌 모양이 없는 블록 (minecraft-data 26.1 blockCollisionShapes).\n\n")
        f.write("pub const NO_COLLISION: &[&str] = &[\n")
        for n in sorted(names):
            f.write(f'    "{n}",\n')
        f.write("];\n")


if __name__ == "__main__":
    main()
