#!/usr/bin/env python3
"""26.3 바닐라 데이터에서 src/data/generated.rs를 만듭니다.

서버는 실행 중에 JSON을 읽지 않습니다. 필요한 것만 이 스크립트로 미리 뽑아 Rust 상수로
굳혀 두고, 생성물은 커밋합니다. 데이터가 바뀌는 건 버전을 올릴 때뿐입니다.

입력 (모두 GitHub에 있습니다):
  --mcmeta-registries  misode/mcmeta, 태그 26.3-registries   (레지스트리 항목 이름)
  --mcmeta-summary     misode/mcmeta, 태그 26.3-summary      (태그 내용)
  --via-mapping        ViaVersion/Mappings, mappings/mapping-26.3.json
                       (블록 상태·블록·아이템·엔티티의 네트워크 ID 순서)

mcmeta의 목록은 이름순이라 내장 레지스트리(block, item, ...)의 네트워크 ID로는 쓸 수
없습니다. 그런 것은 ViaVersion 매핑에서 순서를 가져옵니다.
"""

import argparse
import json
import os
import sys

# 클라이언트가 Configuration 단계에서 반드시 받아야 하는 레지스트리.
# 1.21.11 목록에 26.1(world_clock, *_sound_variant), 26.2(sulfur_cube_archetype),
# 26.3(decorated_pot_pattern, block_transformer, worldgen/block_state_provider)의 추가분을
# 더한 것입니다 — ViaBackwards가 낮은 버전으로 내릴 때 지우는 레지스트리에서 확인했습니다.
SYNCED_REGISTRIES = [
    "banner_pattern",
    "block_transformer",
    "cat_sound_variant",
    "cat_variant",
    "chat_type",
    "chicken_sound_variant",
    "chicken_variant",
    "cow_sound_variant",
    "cow_variant",
    "damage_type",
    "decorated_pot_pattern",
    "dialog",
    "dimension_type",
    "enchantment",
    "frog_variant",
    "instrument",
    "jukebox_song",
    "painting_variant",
    "pig_sound_variant",
    "pig_variant",
    "sulfur_cube_archetype",
    "test_environment",
    "test_instance",
    "timeline",
    "trim_material",
    "trim_pattern",
    "wolf_sound_variant",
    "wolf_variant",
    "world_clock",
    "worldgen/biome",
    "worldgen/block_state_provider",
    "zombie_nautilus_variant",
]

# 네트워크 ID 순서를 ViaVersion 매핑에서 가져오는 내장 레지스트리.
# fluid는 매핑에 없지만 다섯 개뿐이고 Fluids 클래스의 등록 순서가 오래도록 같습니다.
FLUIDS = ["empty", "flowing_water", "water", "flowing_lava", "lava"]

# 월드 생성에 쓰는 블록 상태. (Rust 이름, 상태 문자열)
BLOCK_STATES = [
    ("AIR", "air"),
    ("STONE", "stone"),
    ("GRASS_BLOCK", "grass_block[snowy=false]"),
    ("DIRT", "dirt"),
    ("BEDROCK", "bedrock"),
    ("WATER", "water[level=0]"),
    ("LAVA", "lava[level=0]"),
    ("SAND", "sand"),
    ("GRAVEL", "gravel"),
    ("DEEPSLATE", "deepslate[axis=y]"),
    ("OAK_LOG", "oak_log[axis=y]"),
    ("OAK_LEAVES", "oak_leaves[distance=1,persistent=true,waterlogged=false]"),
    ("SHORT_GRASS", "short_grass"),
    ("NETHERRACK", "netherrack"),
    ("SOUL_SAND", "soul_sand"),
    ("GLOWSTONE", "glowstone"),
    ("END_STONE", "end_stone"),
    ("OBSIDIAN", "obsidian"),
]

# 쓰는 엔티티 종류.
ENTITY_TYPES = [("PLAYER", "player")]

# Commands 패킷의 인자 파서 (`command_argument_type` 레지스트리). 매핑에는 minecraft: 접두사가 빠져 있습니다.
ARGUMENT_TYPES = [
    ("GAME_PROFILE", "game_profile"),
    ("MESSAGE", "message"),
]


def load(path):
    with open(path, encoding="utf-8") as f:
        return json.load(f)


def strip_ns(name):
    return name[len("minecraft:"):] if name.startswith("minecraft:") else name


def resolve_tags(raw, ids):
    """태그 JSON을 ID 목록으로 풉니다. `#다른_태그` 참조와 `required: false`를 처리합니다."""
    resolved = {}

    def walk(name, stack):
        if name in resolved:
            return resolved[name]
        if name in stack:
            sys.exit(f"태그 순환 참조: {' -> '.join(stack + [name])}")
        out = []
        for value in raw[name]["values"]:
            required = True
            if isinstance(value, dict):
                required = value.get("required", True)
                value = value["id"]
            if value.startswith("#"):
                ref = strip_ns(value[1:])
                if ref not in raw:
                    if required:
                        sys.exit(f"없는 태그 참조: {name} -> {value}")
                    continue
                out.extend(walk(ref, stack + [name]))
            else:
                key = strip_ns(value)
                if key not in ids:
                    if required:
                        sys.exit(f"태그 {name}의 항목 {value}가 레지스트리에 없습니다")
                    continue
                out.append(ids[key])
        # 중복 제거, 처음 나온 순서 유지
        seen = set()
        uniq = [i for i in out if not (i in seen or seen.add(i))]
        resolved[name] = uniq
        return uniq

    for name in sorted(raw):
        walk(name, [])
    return resolved


def rust_str(s):
    return json.dumps(s, ensure_ascii=False)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--mcmeta-registries", required=True)
    ap.add_argument("--mcmeta-summary", required=True)
    ap.add_argument("--via-mapping", required=True)
    ap.add_argument("--out", required=True)
    args = ap.parse_args()

    version = load(os.path.join(args.mcmeta_summary, "version.json"))
    if version["protocol_version"] != 777:
        sys.exit(f"26.3(프로토콜 777) 데이터가 아닙니다: {version['id']}")

    mapping = load(args.via_mapping)
    builtin = {
        "block": mapping["blocks"],
        "item": mapping["items"],
        "entity_type": mapping["entities"],
        "fluid": FLUIDS,
    }
    # 매핑과 mcmeta가 같은 버전인지 항목 집합으로 확인합니다.
    for reg, order in builtin.items():
        names = load(os.path.join(args.mcmeta_registries, reg, "data.json"))
        if sorted(names) != sorted(order):
            sys.exit(f"{reg}: ViaVersion 매핑과 mcmeta 항목이 다릅니다")

    synced = {}
    for reg in SYNCED_REGISTRIES:
        path = os.path.join(args.mcmeta_registries, reg, "data.json")
        if not os.path.exists(path):
            sys.exit(f"레지스트리가 없습니다: {reg}")
        synced[reg] = load(path)

    states = mapping["blockstates"]
    state_ids = {s: i for i, s in enumerate(states)}

    # 태그: 내장 레지스트리 + 동기화 레지스트리 중 summary에 태그가 있는 것.
    tag_root = os.path.join(args.mcmeta_summary, "data", "tag")
    tags = {}
    for reg in list(builtin) + SYNCED_REGISTRIES:
        path = os.path.join(tag_root, reg, "data.json")
        if not os.path.exists(path):
            continue
        order = builtin.get(reg) or synced[reg]
        ids = {name: i for i, name in enumerate(order)}
        tags[reg] = resolve_tags(load(path), ids)

    out = []
    w = out.append
    w("// @generated by tools/gen_data.py — 손으로 고치지 마세요.")
    w(f"// Minecraft {version['id']} (protocol {version['protocol_version']}, data {version['data_version']})")
    w("")
    w("/// 블록 상태 전체 개수. 직접 팔레트의 비트 수를 정합니다.")
    w(f"pub const BLOCK_STATE_COUNT: u32 = {len(states)};")
    w("")
    w("/// 월드 생성에 쓰는 블록 상태 ID.")
    w("pub mod block {")
    for name, state in BLOCK_STATES:
        if state not in state_ids:
            sys.exit(f"블록 상태가 없습니다: {state}")
        w(f"    /// `{state}`")
        w(f"    pub const {name}: u32 = {state_ids[state]};")
    w("}")
    w("")
    w("/// 엔티티 종류의 네트워크 ID.")
    w("pub mod entity_type {")
    for name, key in ENTITY_TYPES:
        w(f"    pub const {name}: i32 = {mapping['entities'].index(key)};")
    w("}")
    w("")
    w("/// 명령어 인자 파서의 네트워크 ID.")
    w("pub mod argument_type {")
    for name, key in ARGUMENT_TYPES:
        w(f"    pub const {name}: i32 = {mapping['argumenttypes'].index(key)};")
    w("}")
    w("")
    w("/// Configuration 단계에서 보내는 레지스트리와 그 항목. 항목 순서가 곧 네트워크 ID입니다.")
    w("pub const SYNCED_REGISTRIES: &[(&str, &[&str])] = &[")
    for reg in SYNCED_REGISTRIES:
        entries = ", ".join(rust_str(e) for e in synced[reg])
        w(f"    ({rust_str(reg)}, &[{entries}]),")
    w("];")
    w("")
    w("/// 레지스트리별 태그. ID는 위 순서(동기화 레지스트리) 또는 바닐라 내장 순서입니다.")
    w("pub const TAGS: &[(&str, &[(&str, &[u32])])] = &[")
    for reg, entries in tags.items():
        w(f"    ({rust_str(reg)}, &[")
        for name in sorted(entries):
            ids = ", ".join(str(i) for i in entries[name])
            w(f"        ({rust_str(name)}, &[{ids}]),")
        w("    ]),")
    w("];")
    w("")

    with open(args.out, "w", encoding="utf-8") as f:
        f.write("\n".join(out))
    print(f"{args.out}: 레지스트리 {len(synced)}개, 태그 레지스트리 {len(tags)}개, 블록 상태 {len(states)}개")


if __name__ == "__main__":
    main()
