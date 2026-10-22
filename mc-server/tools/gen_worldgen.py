#!/usr/bin/env python3
"""바닐라 데이터에서 구조물 배치·구조물 바이옴·상자 전리품 표를 Rust 상수로 만듭니다.

입력은 misode/mcmeta `26.3-data` 브랜치에서 받은 파일들:
  <wg>/structure_set/*.json        구조물 묶음과 배치 (간격·분리·소금, 동심원)
  <wg>/structure_biomes.json       구조물 → 바이옴 목록 (has_structure 태그를 펼친 것)
  <wg>/loot/chests/**/*.json       상자 전리품 표

사용:
  python3 -I tools/gen_worldgen.py --wg <wg 폴더> --out src/world/structure/data.rs
"""

import argparse
import glob
import json
import os


def rust_str(s):
    return '"' + s.replace("\\", "\\\\").replace('"', '\\"') + '"'


def strip(n):
    return n.split(":", 1)[1] if ":" in n else n


def int_range(v):
    """숫자 또는 {uniform,min,max} / {binomial,n,p} → (최소, 최대)."""
    if isinstance(v, (int, float)):
        return int(v), int(v)
    t = strip(v.get("type", "uniform"))
    if t == "uniform":
        return int(int_range(v["min"])[0]), int(int_range(v["max"])[1])
    if t == "binomial":
        return 0, int(int_range(v["n"])[1])
    if t == "constant":
        return int(v["value"]), int(v["value"])
    return 1, 1


def modifiers(e):
    for k in ("modifier", "modifiers", "functions"):
        if k in e:
            v = e[k]
            yield from (v if isinstance(v, list) else [v])


def entry(e):
    """(아이템, 가중치, 최소, 최대, 마법 부여) — 빈 칸은 아이템 ""."""
    t = strip(e["type"])
    w = int(e.get("weight", 1))
    if t == "empty":
        return ("", w, 0, 0, False)
    if t != "item":
        return None
    lo, hi = 1, 1
    enchanted = False
    for m in modifiers(e):
        mt = strip(m.get("type", m.get("function", "")))
        if mt == "set_count":
            lo, hi = int_range(m["count"])
        elif mt in ("enchant_randomly", "enchant_with_levels", "set_enchantments"):
            enchanted = True
    return (strip(e["name"]), w, lo, hi, enchanted)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--wg", required=True)
    ap.add_argument("--out", required=True)
    a = ap.parse_args()

    out = []
    w = out.append
    w("//! tools/gen_worldgen.py가 만든 파일입니다. 손으로 고치지 마세요.")
    w("//! 출처: misode/mcmeta 26.3-data (worldgen/structure_set, tags/worldgen/biome/has_structure, loot_table/chests)")
    w("")
    w("use super::{LootPool, Placement, SetData};")
    w("")

    w("/// 바닐라 구조물 묶음과 배치. `structures`는 (구조물, 가중치).")
    w("pub const STRUCTURE_SETS: &[SetData] = &[")
    for f in sorted(glob.glob(os.path.join(a.wg, "structure_set", "*.json"))):
        name = os.path.basename(f)[:-5]
        d = json.load(open(f))
        p = d["placement"]
        t = strip(p["type"])
        if t == "random_spread":
            freq = float(p.get("frequency", 1.0))
            triangular = strip(p.get("spread_type", "linear")) == "triangular"
            pl = (f"Placement::Spread {{ spacing: {p['spacing']}, separation: {p['separation']}, "
                  f"salt: {p['salt']}, frequency: {freq:.4f}, triangular: {str(triangular).lower()} }}")
        elif t == "concentric_rings":
            pl = (f"Placement::Rings {{ distance: {p['distance']}, count: {p['count']}, "
                  f"spread: {p['spread']} }}")
        else:
            continue
        structs = ", ".join(f"({rust_str(strip(s['structure']))}, {s.get('weight', 1)})" for s in d["structures"])
        w(f"    SetData {{ name: {rust_str(name)}, placement: {pl}, structures: &[{structs}] }},")
    w("];")
    w("")

    sb = json.load(open(os.path.join(a.wg, "structure_biomes.json")))
    w("/// 구조물이 생길 수 있는 바이옴.")
    w("pub const STRUCTURE_BIOMES: &[(&str, &[&str])] = &[")
    for k in sorted(sb):
        w(f"    ({rust_str(k)}, &[" + ", ".join(rust_str(b) for b in sb[k]) + "]),")
    w("];")
    w("")

    w("/// 상자 전리품 표. 칸: (아이템, 가중치, 최소, 최대, 마법 부여). 아이템 \"\" = 빈 칸.")
    w("pub const LOOT_TABLES: &[(&str, &[LootPool])] = &[")
    for f in sorted(glob.glob(os.path.join(a.wg, "loot", "chests", "**", "*.json"), recursive=True)):
        rel = os.path.relpath(f, os.path.join(a.wg, "loot"))[:-5].replace(os.sep, "/")
        t = json.load(open(f))
        pools = []
        for p in t.get("pools", []):
            lo, hi = int_range(p["rolls"])
            es = [x for x in (entry(e) for e in p["entries"]) if x]
            if not es:
                continue
            body = ", ".join(
                f"({rust_str(i)}, {wt}, {mn}, {mx}, {str(en).lower()})" for i, wt, mn, mx, en in es)
            pools.append(f"LootPool {{ rolls: ({lo}, {hi}), entries: &[{body}] }}")
        w(f"    ({rust_str(rel)}, &[")
        for p in pools:
            w(f"        {p},")
        w("    ]),")
    w("];")

    with open(a.out, "w") as fh:
        fh.write("\n".join(out) + "\n")
    print(f"{a.out}: 구조물 묶음, 바이옴 {len(sb)}개, 전리품 표")


if __name__ == "__main__":
    main()
