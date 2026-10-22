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

# 아이템 이름과 놓이는 블록 이름이 다른 것들. 나머지는 이름이 같으면 그 블록을 놓습니다.
ITEM_BLOCK_EXCEPTIONS = {
    "redstone": "redstone_wire",
    "string": "tripwire",
    "wheat_seeds": "wheat",
    "beetroot_seeds": "beetroots",
    "carrot": "carrots",
    "potato": "potatoes",
    "melon_seeds": "melon_stem",
    "pumpkin_seeds": "pumpkin_stem",
    "sweet_berries": "sweet_berry_bush",
    "glow_berries": "cave_vines",
    "cocoa_beans": "cocoa",
    "torchflower_seeds": "torchflower_crop",
    "pitcher_pod": "pitcher_crop",
    "water_bucket": "water",
    "lava_bucket": "lava",
    "powder_snow_bucket": "powder_snow",
}

# 쓰는 엔티티 종류.
ENTITY_TYPES = [
    ("PLAYER", "player"),
    ("LIGHTNING_BOLT", "lightning_bolt"),
    ("ITEM", "item"),
    ("COW", "cow"),
    ("PIG", "pig"),
    ("SHEEP", "sheep"),
    ("CHICKEN", "chicken"),
    ("ZOMBIE", "zombie"),
    ("SKELETON", "skeleton"),
    ("CREEPER", "creeper"),
    ("SPIDER", "spider"),
]

# 아이템 컴포넌트 (`data_component_type`): 사용자 정의 아이템에 씁니다.
COMPONENTS = [
    ("CUSTOM_DATA", "custom_data"),
    ("MAX_STACK_SIZE", "max_stack_size"),
    ("CUSTOM_NAME", "custom_name"),
    ("ITEM_NAME", "item_name"),
    ("ITEM_MODEL", "item_model"),
    ("LORE", "lore"),
    ("RARITY", "rarity"),
    ("ENCHANTMENT_GLINT_OVERRIDE", "enchantment_glint_override"),
    ("FOOD", "food"),
]

# 소리 (`sound_event` 레지스트리).
SOUNDS = [
    ("THUNDER", "entity.lightning_bolt.thunder"),
    ("TELEPORT", "entity.enderman.teleport"),
    ("LEVEL_UP", "entity.player.levelup"),
    ("LAUNCH", "entity.firework_rocket.launch"),
    ("EXPERIENCE_ORB", "entity.experience_orb.pickup"),
]

# 내용 없는 입자 (`particle_type`).
PARTICLES = [
    ("HEART", "heart"),
    ("PORTAL", "portal"),
    ("CLOUD", "cloud"),
    ("END_ROD", "end_rod"),
    ("CRIT", "crit"),
    ("HAPPY_VILLAGER", "happy_villager"),
    ("ELECTRIC_SPARK", "electric_spark"),
]

# 열 수 있는 창 종류 (`menu` 레지스트리).
MENUS = [
    ("GENERIC_9X3", "generic_9x3"), ("GENERIC_9X6", "generic_9x6"), ("GENERIC_3X3", "generic_3x3"), ("CRAFTING", "crafting"),
    ("FURNACE", "furnace"), ("BLAST_FURNACE", "blast_furnace"), ("SMOKER", "smoker"),
    ("ENCHANTMENT", "enchantment"), ("ANVIL", "anvil"), ("STONECUTTER", "stonecutter"),
    ("SMITHING", "smithing"), ("GRINDSTONE", "grindstone"), ("MERCHANT", "merchant"),
    ("HOPPER", "hopper"), ("BREWING_STAND", "brewing_stand"),
]


def tool_conditioned(entry):
    """실크 터치·가위가 있어야만 나오는 항목."""
    text = json.dumps(entry.get("condition", ""))
    return "can_silk_touch" in text or "can_shear" in text


def simple_drop(table, item_ids):
    """블록 전리품 표에서 '맨손·일반 도구로 부쉈을 때' 나오는 첫 아이템만 뽑습니다.
    (아이템, 최소, 최대, 확률 천분율) 또는 None. 행운·폭발 감쇠 같은 수정자는 무시합니다."""
    pools = table.get("pools") or []
    if not pools or not pools[0].get("entries"):
        return None

    def pick(e):
        t = e.get("type")
        if t == "minecraft:alternatives":
            for c in e["children"]:
                if not tool_conditioned(c):
                    return pick(c)
            return None
        if t == "minecraft:item" and not tool_conditioned(e):
            return e
        return None

    e = pick(pools[0]["entries"][0])
    if e is None:
        return None
    item = strip_ns(e["name"])
    if item not in item_ids:
        return None
    lo = hi = 1
    for f in e.get("functions", []) + pools[0].get("functions", []):
        if strip_ns(f.get("function", "")) == "set_count":
            c = f["count"]
            if isinstance(c, (int, float)):
                lo = hi = int(c)
            elif isinstance(c, dict) and "min" in c and "max" in c:
                lo, hi = int(c["min"]), int(c["max"])
    chance = 1000
    conds = []
    for holder in (e, pools[0]):
        c = holder.get("condition") or holder.get("conditions")
        if isinstance(c, list):
            conds += c
        elif c is not None:
            conds.append(c)
    flat = []
    for c in conds:
        if isinstance(c, dict) and strip_ns(c.get("type", "")) == "all_of":
            flat += c.get("terms", [])
        else:
            flat.append(c)
    for c in flat:
        if isinstance(c, dict):
            t = strip_ns(c.get("type", ""))
            if t == "random_chance" and isinstance(c.get("chance"), (int, float)):
                chance = int(c["chance"] * 1000)
            elif t == "table_bonus":
                chance = int(c["chances"][0] * 1000)
    if hi <= 0:
        return None
    return (item_ids[item], max(lo, 0), hi, chance)

# Commands 패킷의 인자 파서 (`command_argument_type` 레지스트리). 매핑에는 minecraft: 접두사가 빠져 있습니다.
ARGUMENT_TYPES = [
    ("GAME_PROFILE", "game_profile"),
    ("MESSAGE", "message"),
    ("ITEM_STACK", "item_stack"),
    ("INTEGER", "brigadier:integer"),
    ("STRING", "brigadier:string"),
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

    # 블록마다 (첫 상태 ID, 기본 상태 ID, 속성들). 상태 ID는 속성(이름순)의 데카르트 곱으로,
    # 마지막 속성이 가장 빨리 바뀌고 값은 선언 순서입니다. 그 공식이 실제 목록과 맞는지
    # 블록 상태 전부를 다시 만들어 확인합니다. (ViaVersion 문자열의 속성 순서는 상자·피스톤
    # 머리 등에서 이름순이 아니므로, 비교는 순서와 무관하게 합니다.)
    block_props = load(os.path.join(args.mcmeta_summary, "blocks", "data.json"))
    blocks = []
    next_state = 0
    for name in mapping["blocks"]:
        props, default = block_props[name]
        keys = sorted(props)
        combos = [[]]
        for k in keys:
            combos = [c + [(k, v)] for c in combos for v in props[k]]
        for i, combo in enumerate(combos):
            actual = states[next_state + i]
            actual_name, _, rest = actual.partition("[")
            actual_props = sorted(rest.rstrip("]").split(",")) if rest else []
            if actual_name != name or actual_props != sorted(f"{k}={v}" for k, v in combo):
                sys.exit(f"상태 순서 불일치: {actual} != {name}{combo}")
        default_index = combos.index([(k, default[k]) for k in keys])
        blocks.append((name, next_state, next_state + default_index, [(k, props[k]) for k in keys]))
        next_state += len(combos)
    if next_state != len(states):
        sys.exit("블록 상태 개수 불일치")

    block_ids = {name: i for i, name in enumerate(mapping["blocks"])}
    item_block = []
    for item in mapping["items"]:
        target = ITEM_BLOCK_EXCEPTIONS.get(item, item)
        item_block.append(block_ids.get(target, -1) if item != "air" else -1)

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
    w("/// 블록 하나: 이름, 첫 상태 ID, 기본 상태 ID, 속성(이름순)과 값(선언순).")
    w("/// 상태 ID = 첫 상태 + Σ 값 번호 × (뒤 속성들의 값 개수 곱).")
    w("pub const BLOCKS: &[(&str, u32, u32, &[(&str, &[&str])])] = &[")
    for name, first, default, props in blocks:
        ps = ", ".join(
            f"({rust_str(k)}, &[{', '.join(rust_str(v) for v in vs)}])" for k, vs in props
        )
        w(f"    ({rust_str(name)}, {first}, {default}, &[{ps}]),")
    w("];")
    w("")
    w("/// 아이템 ID → 놓았을 때의 블록 ID (-1 = 블록이 아님).")
    w("pub const ITEM_BLOCK: &[i16] = &[")
    for i in range(0, len(item_block), 32):
        w("    " + ", ".join(str(b) for b in item_block[i:i + 32]) + ",")
    w("];")
    w("")
    # ---- 아이템 성질 ----
    components = load(os.path.join(args.mcmeta_summary, "item_components", "data.json"))
    item_ids = {name: i for i, name in enumerate(mapping["items"])}
    w("/// 아이템별 (최대 개수, 음식 (회복량, 포만도) — 음식이 아니면 회복량 0, 공격력).")
    w("/// 공격력은 맨손 1에 아이템의 attack_damage 수정치를 더한 값입니다.")
    w("pub const ITEMS: &[(u8, u8, f32, f32)] = &[")
    for name in mapping["items"]:
        c = components.get(name, {})
        stack = c.get("minecraft:max_stack_size", 64)
        food = c.get("minecraft:food") or {}
        attack = 1.0 + sum(
            m["amount"] for m in c.get("minecraft:attribute_modifiers", [])
            if strip_ns(m.get("type", "")) == "attack_damage" and m.get("slot") in ("mainhand", None)
        )
        w(f"    ({stack}, {food.get('nutrition', 0)}, {float(food.get('saturation', 0.0))}, {float(attack)}),")
    w("];")
    w("")

    # ---- 조합법 (작업대·2×2) ----
    recipes = load(os.path.join(args.mcmeta_summary, "data", "recipe", "data.json"))
    item_tags = tags.get("item", {})

    def ingredient(spec):
        if isinstance(spec, list):
            return sorted({i for x in spec for i in ingredient(x)})
        if spec.startswith("#"):
            return sorted(item_tags.get(strip_ns(spec[1:]), []))
        key = strip_ns(spec)
        return [item_ids[key]] if key in item_ids else []

    out_recipes = []
    for rid in sorted(recipes):
        r = recipes[rid]
        kind = strip_ns(r["type"])
        res = r.get("result", {})
        rname = strip_ns(res.get("id", ""))
        if rname not in item_ids:
            continue
        if kind == "crafting_shaped":
            pattern = r["pattern"]
            h, wdt = len(pattern), max(len(row) for row in pattern)
            cells = []
            ok = True
            for row in pattern:
                for x in range(wdt):
                    ch = row[x] if x < len(row) else " "
                    if ch == " ":
                        cells.append(None)
                    else:
                        ing = ingredient(r["key"][ch])
                        ok &= bool(ing)
                        cells.append(ing)
            if ok:
                out_recipes.append((True, wdt, h, cells, item_ids[rname], res.get("count", 1)))
        elif kind == "crafting_shapeless":
            cells = [ingredient(x) for x in r["ingredients"]]
            if all(cells):
                out_recipes.append((False, len(cells), 1, cells, item_ids[rname], res.get("count", 1)))
    # ---- 굽기 (화로·용광로·훈연기·모닥불) ----
    cook_kinds = {"smelting": 0, "blasting": 1, "smoking": 2, "campfire_cooking": 3}
    w("/// 굽기 조리법: (종류 0 화로·1 용광로·2 훈연기·3 모닥불, 재료들, 결과, 개수, 틱, 경험치).")
    w("pub const COOKING: &[(u8, &[u16], u16, u8, u16, f32)] = &[")
    for rid in sorted(recipes):
        r = recipes[rid]
        kind = strip_ns(r["type"])
        if kind not in cook_kinds:
            continue
        res = r.get("result", {})
        rname = strip_ns(res.get("id", ""))
        ing = ingredient(r["ingredient"])
        if rname not in item_ids or not ing:
            continue
        w(f"    ({cook_kinds[kind]}, &[{', '.join(map(str, ing))}], {item_ids[rname]}, {res.get('count', 1)}, "
          f"{r.get('cookingtime', 200)}, {float(r.get('experience', 0.0))}),")
    w("];")
    w("")
    # ---- 석재 절단기 ----
    w("/// 석재 절단기: (재료들, 결과, 개수).")
    w("pub const STONECUTTING: &[(&[u16], u16, u8)] = &[")
    for rid in sorted(recipes):
        r = recipes[rid]
        if strip_ns(r["type"]) != "stonecutting":
            continue
        res = r.get("result", {})
        rname = strip_ns(res.get("id", ""))
        ing = ingredient(r["ingredient"])
        if rname in item_ids and ing:
            w(f"    (&[{', '.join(map(str, ing))}], {item_ids[rname]}, {res.get('count', 1)}),")
    w("];")
    w("")
    w("/// 조합법: (모양 있음, 너비, 높이, 칸별 재료(없으면 빈 칸), 결과 아이템, 개수).")
    w("/// 모양 없는 조합법은 너비 = 재료 수, 높이 1.")
    w("pub const RECIPES: &[(bool, u8, u8, &[Option<&[u16]>], u16, u8)] = &[")
    for shaped, wdt, h, cells, result, count in out_recipes:
        cs = ", ".join("None" if c is None else "Some(&[" + ", ".join(map(str, c)) + "])" for c in cells)
        w(f"    ({str(shaped).lower()}, {wdt}, {h}, &[{cs}], {result}, {count}),")
    w("];")
    w("")

    # ---- 블록 전리품 ----
    loot = load(os.path.join(args.mcmeta_summary, "data", "loot_table", "data.json"))
    w("/// 블록 ID → 부쉈을 때 떨어지는 것 (아이템, 최소, 최대, 확률 천분율). 없으면 -1 아이템.")
    w("pub const BLOCK_DROPS: &[(i16, u8, u8, u16)] = &[")
    for name in mapping["blocks"]:
        d = simple_drop(loot.get(f"blocks/{name}", {}), item_ids)
        w(f"    ({d[0]}, {d[1]}, {d[2]}, {d[3]})," if d else "    (-1, 0, 0, 0),")
    w("];")
    w("")
    for mod, table, key in [("component", COMPONENTS, "data_component_type"), ("sound", SOUNDS, "sounds"), ("particle", PARTICLES, "particles")]:
        w(f"/// `{key}` 네트워크 ID.")
        w(f"pub mod {mod} {{")
        for name, k in table:
            w(f"    pub const {name}: i32 = {mapping[key].index(k)};")
        w("}")
        w("")
    w("/// 열 수 있는 창 종류의 네트워크 ID.")
    w("pub mod menu {")
    for name, key in MENUS:
        w(f"    pub const {name}: i32 = {mapping['menus'].index(key)};")
    w("}")
    w("")
    w("/// 아이템 이름 → ID (명령어, 도구 판별용). 순서 = 네트워크 ID.")
    w("pub const ITEM_NAMES: &[&str] = &[" + ", ".join(rust_str(n) for n in mapping["items"]) + "];")
    w("")
    w("/// 블록 엔티티 종류 이름. 순서 = 네트워크 ID (청크 패킷의 블록 엔티티 목록).")
    w("pub const BLOCK_ENTITY_TYPES: &[&str] = &[" + ", ".join(rust_str(n) for n in mapping["blockentities"]) + "];")
    w("")
    w("/// 소리 이름. 순서 = 네트워크 ID.")
    w("pub const SOUND_NAMES: &[&str] = &[" + ", ".join(rust_str(n) for n in mapping["sounds"]) + "];")
    w("")
    w("/// 입자 이름. 순서 = 네트워크 ID.")
    w("pub const PARTICLE_NAMES: &[&str] = &[" + ", ".join(rust_str(n) for n in mapping["particles"]) + "];")
    w("")
    w("/// 엔티티 종류 이름. 순서 = 네트워크 ID.")
    w("pub const ENTITY_NAMES: &[&str] = &[" + ", ".join(rust_str(n) for n in mapping["entities"]) + "];")
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
