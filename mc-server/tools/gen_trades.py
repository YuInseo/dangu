#!/usr/bin/env python3
"""바닐라 주민 거래표(26.3 `villager_trade` 레지스트리)를 Rust 상수로 만듭니다.

입력은 misode/mcmeta `26.3-data` 브랜치의
  data/minecraft/villager_trade/**/*.json        거래 하나하나
  data/minecraft/tags/villager_trade/**/*.json   직업/레벨별 묶음 (level_1 … level_5)

아이템을 바꾸는 수정자(마법 부여, 탐험 지도, 수상한 스튜 …)가 붙은 거래는 그 수정자를
옮기지 않았으므로 빼고, 주민 종류(바이옴)에 따라 갈리는 거래는 종류 목록을 함께 적습니다.

사용:
  python3 -I tools/gen_trades.py --trades <villager_trade 폴더> --tags <tags/villager_trade 폴더> \
      --out src/data/trades.rs
"""

import argparse
import json
import os

VARIANTS = ["desert", "jungle", "plains", "savanna", "snow", "swamp", "taiga"]


def strip(n):
    return n.split(":", 1)[1] if ":" in n else n


def rust_str(s):
    return '"' + s + '"'


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--trades", required=True)
    ap.add_argument("--tags", required=True)
    ap.add_argument("--out", required=True)
    a = ap.parse_args()

    def load_tag(name, seen=()):
        path = os.path.join(a.tags, name + ".json")
        out = []
        for v in json.load(open(path))["values"]:
            if isinstance(v, dict):
                v = v["id"]
            if v.startswith("#"):
                t = strip(v[1:])
                if t not in seen:
                    out += load_tag(t, seen + (t,))
            else:
                out.append(strip(v))
        return out

    def stack(s):
        return (strip(s["id"]), int(s.get("count", 1)))

    lines = []
    skipped = 0
    for prof in sorted(os.listdir(a.tags)):
        if prof in ("common_smith", "wandering_trader"):
            continue
        for level in range(1, 6):
            if not os.path.exists(os.path.join(a.tags, prof, f"level_{level}.json")):
                continue
            entries = []
            for t in load_tag(f"{prof}/level_{level}"):
                d = json.load(open(os.path.join(a.trades, t + ".json")))
                if "given_item_modifier" in d or "components" in d["wants"]:
                    skipped += 1
                    continue
                mask = 0
                pred = d.get("merchant_predicate")
                if pred:
                    vs = pred.get("predicate", {}).get("minecraft:predicates", {}).get(
                        "minecraft:villager/variant"
                    )
                    if not vs:
                        skipped += 1
                        continue
                    if isinstance(vs, str):
                        vs = [vs]
                    for v in vs:
                        mask |= 1 << VARIANTS.index(strip(v))
                wants, gives = stack(d["wants"]), stack(d["gives"])
                extra = stack(d["additional_wants"]) if "additional_wants" in d else None
                extra_s = f"Some(({rust_str(extra[0])}, {extra[1]}))" if extra else "None"
                entries.append(
                    f"        Trade {{ wants: ({rust_str(wants[0])}, {wants[1]}), extra: {extra_s}, "
                    f"gives: ({rust_str(gives[0])}, {gives[1]}), max_uses: {int(d.get('max_uses', 4))}, "
                    f"xp: {int(d.get('xp', 1))}, price_multiplier: {float(d.get('reputation_discount', 0.0))!r}, "
                    f"variants: {mask} }},"
                )
            lines.append(f"    ({rust_str(prof)}, {level}, &[\n" + "\n".join(entries) + "\n    ]),")

    with open(a.out, "w") as f:
        f.write("// tools/gen_trades.py가 만든 파일입니다. 손으로 고치지 마세요.\n")
        f.write(f"// 바닐라 26.3 주민 거래 (아이템 수정자가 필요한 {skipped}개는 뺐습니다).\n\n")
        f.write("#[derive(Debug, Clone, Copy, PartialEq)]\n")
        f.write("pub struct Trade {\n")
        f.write("    pub wants: (&'static str, u8),\n")
        f.write("    pub extra: Option<(&'static str, u8)>,\n")
        f.write("    pub gives: (&'static str, u8),\n")
        f.write("    pub max_uses: u8,\n")
        f.write("    pub xp: u8,\n")
        f.write("    pub price_multiplier: f32,\n")
        f.write("    /// 이 거래를 내놓는 주민 종류 (비트 = [`VARIANTS`] 순서). 0이면 모두.\n")
        f.write("    pub variants: u8,\n")
        f.write("}\n\n")
        f.write("pub const VARIANTS: [&str; 7] = [" + ", ".join(rust_str(v) for v in VARIANTS) + "];\n\n")
        f.write("/// (직업, 레벨 1~5, 거래).\n")
        f.write("pub const TRADES: &[(&str, u8, &[Trade])] = &[\n")
        f.write("\n".join(lines))
        f.write("\n];\n")


if __name__ == "__main__":
    main()
