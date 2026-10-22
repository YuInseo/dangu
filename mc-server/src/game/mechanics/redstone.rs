//! 기본 레드스톤.
//!
//! - 전원: 레버, 버튼(눌린 동안), 압력판(밟는 동안), 레드스톤 블록, 레드스톤 횃불, 중계기 출력.
//! - 전선: 레드스톤 가루. 전원에서 15로 시작해 한 칸마다 1씩 약해지고, 계단처럼 한 칸 위아래로
//!   이어집니다. 모양(옆·위 연결)도 맞춥니다.
//! - 받는 쪽: 레드스톤 램프, 문·다락문·울타리 문(나무·철), TNT, 레드스톤 횃불(붙은 블록에 신호가
//!   오면 꺼짐), 중계기(뒤에서 받아 앞으로, 지연 1~4).
//! - 블록 전달: 전원이 붙은 블록, 횃불 위 블록, 중계기가 가리키는 블록은 "강하게" 켜져 옆의
//!   가루와 받는 쪽에 신호를 줍니다.
//!
//! 피스톤·비교기·관측기는 아직 없습니다.

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Mutex;

use super::{NEIGHBORS, add};
use crate::data::blocks;
use crate::packets::world::BlockPos;
use crate::server::Server;
use crate::world::Dimension;
use crate::world::blockset::name_of;

const HORIZONTAL: [((i32, i32, i32), &str); 4] = [
    ((0, 0, -1), "north"),
    ((1, 0, 0), "east"),
    ((0, 0, 1), "south"),
    ((-1, 0, 0), "west"),
];

/// 지연이 있는 것들의 예정 시각 (중계기, 버튼, 압력판).
#[derive(Default)]
pub struct State {
    due: Mutex<HashMap<(Dimension, BlockPos), u64>>,
}

pub fn is_component(n: &str) -> bool {
    matches!(
        n,
        "redstone_wire"
            | "lever"
            | "redstone_block"
            | "redstone_torch"
            | "redstone_wall_torch"
            | "repeater"
            | "redstone_lamp"
            | "tnt"
    ) || n.ends_with("_button")
        || n.ends_with("_door")
        || n.ends_with("_trapdoor")
        || n.ends_with("fence_gate")
        || n.ends_with("pressure_plate")
}

fn dir_vec(name: &str) -> (i32, i32, i32) {
    match name {
        "north" => (0, 0, -1),
        "south" => (0, 0, 1),
        "east" => (1, 0, 0),
        "west" => (-1, 0, 0),
        "up" => (0, 1, 0),
        _ => (0, -1, 0),
    }
}

fn prop<'a>(state: u32, k: &str) -> Option<&'a str> {
    blocks::property(state, k)
}

fn on(state: u32, k: &str) -> bool {
    prop(state, k) == Some("true")
}

/// 레버·버튼이 붙어 있는 블록 쪽.
fn attached_dir(state: u32) -> (i32, i32, i32) {
    match prop(state, "face") {
        Some("floor") => (0, -1, 0),
        Some("ceiling") => (0, 1, 0),
        _ => {
            let f = dir_vec(prop(state, "facing").unwrap_or("north"));
            (-f.0, -f.1, -f.2)
        }
    }
}

fn is_solid(state: u32) -> bool {
    !crate::world::features::soft(state)
        && !is_component(name_of(state))
        && state != crate::data::block::LAVA
}

/// `src`에 있는 블록이 `d` 방향(→ 받는 쪽)으로 주는 신호 (가루 제외).
fn emits(state: u32, d: (i32, i32, i32)) -> u8 {
    let n = name_of(state);
    match n {
        "redstone_block" => 15,
        "lever" if on(state, "powered") => 15,
        _ if n.ends_with("_button") && on(state, "powered") => 15,
        _ if n.ends_with("pressure_plate")
            && (on(state, "powered") || prop(state, "power").is_some_and(|p| p != "0")) =>
        {
            15
        }
        "redstone_torch" if on(state, "lit") => {
            if d == (0, -1, 0) {
                0
            } else {
                15
            }
        }
        "redstone_wall_torch" if on(state, "lit") => {
            let f = dir_vec(prop(state, "facing").unwrap_or("north"));
            if d == (-f.0, -f.1, -f.2) { 0 } else { 15 }
        }
        "repeater" if on(state, "powered") => {
            // facing은 입력 쪽, 출력은 반대
            let f = dir_vec(prop(state, "facing").unwrap_or("north"));
            if d == (-f.0, -f.1, -f.2) { 15 } else { 0 }
        }
        _ => 0,
    }
}

/// 고체 블록이 강하게 켜져 있는가 (붙은 레버·버튼, 아래 횃불, 가리키는 중계기).
fn strongly_powered(server: &Server, dim: Dimension, pos: BlockPos) -> bool {
    for d in NEIGHBORS {
        let n = add(pos, d);
        let s = server.world.get(dim, n.0, n.1, n.2);
        let name = name_of(s);
        let toward = (-d.0, -d.1, -d.2);
        let attached_here =
            (name == "lever" || name.ends_with("_button")) && attached_dir(s) == toward;
        if attached_here && emits(s, toward) > 0 {
            return true;
        }
        if name == "redstone_torch" && d == (0, -1, 0) && on(s, "lit") {
            return true;
        }
        if name == "repeater" && emits(s, toward) > 0 {
            return true;
        }
    }
    false
}

/// 가루가 아닌 받는 쪽에 오는 신호 세기 (0이면 꺼짐).
fn power_at(server: &Server, dim: Dimension, pos: BlockPos) -> u8 {
    let mut best = 0;
    for d in NEIGHBORS {
        let n = add(pos, d);
        let s = server.world.get(dim, n.0, n.1, n.2);
        let toward = (-d.0, -d.1, -d.2);
        best = best.max(emits(s, toward));
        if name_of(s) == "redstone_wire" && d != (0, -1, 0) {
            best = best.max(wire_power(s));
        }
        if is_solid(s) && strongly_powered(server, dim, n) {
            best = 15;
        }
        // 고체 블록 위의 가루는 그 블록을 약하게 켜고, 그 블록 옆의 받는 쪽에 전해짐
        if is_solid(s) {
            let above = server.world.get(dim, n.0, n.1 + 1, n.2);
            if name_of(above) == "redstone_wire" && wire_power(above) > 0 && d != (0, -1, 0) {
                best = best.max(1);
            }
        }
    }
    best
}

fn wire_power(state: u32) -> u8 {
    prop(state, "power")
        .and_then(|p| p.parse().ok())
        .unwrap_or(0)
}

/// 이웃에 바뀐 것이 있으면 근처 가루를 다시 계산하도록 예약 (대각선 포함).
pub fn nearby_changed(server: &Server, dim: Dimension, pos: BlockPos) {
    for dx in -1..=1 {
        for dy in -1..=1 {
            for dz in -1..=1 {
                if dx == 0 && dy == 0 && dz == 0 {
                    continue;
                }
                let p = (pos.0 + dx, pos.1 + dy, pos.2 + dz);
                if name_of(server.world.get(dim, p.0, p.1, p.2)) == "redstone_wire" {
                    server.ticks.schedule(dim, p, 1);
                }
            }
        }
    }
}

/// 예약 틱 하나: 그 자리의 부품을 다시 계산합니다.
pub fn update(server: &Server, dim: Dimension, pos: BlockPos) {
    let state = server.world.get(dim, pos.0, pos.1, pos.2);
    let n = name_of(state);
    let now = server.ticks.now();
    match n {
        "redstone_wire" => update_wires(server, dim, pos),
        "redstone_lamp" => {
            let lit = power_at(server, dim, pos) > 0;
            if on(state, "lit") != lit {
                server.set_block(
                    dim,
                    pos,
                    blocks::with_property(state, "lit", &lit.to_string()),
                );
            }
        }
        "tnt" => {
            if power_at(server, dim, pos) > 0 {
                super::explosion::prime(server, dim, pos);
            }
        }
        "redstone_torch" | "redstone_wall_torch" => {
            let back = if n == "redstone_torch" {
                (0, -1, 0)
            } else {
                let f = dir_vec(prop(state, "facing").unwrap_or("north"));
                (-f.0, -f.1, -f.2)
            };
            let attached = add(pos, back);
            let powered = strongly_powered(server, dim, attached)
                || power_into_block(server, dim, attached, pos);
            let lit = !powered;
            if on(state, "lit") != lit {
                server.set_block(
                    dim,
                    pos,
                    blocks::with_property(state, "lit", &lit.to_string()),
                );
            }
        }
        "repeater" => {
            let f = dir_vec(prop(state, "facing").unwrap_or("north"));
            let back = add(pos, f);
            let bs = server.world.get(dim, back.0, back.1, back.2);
            let input = emits(bs, (-f.0, -f.1, -f.2)) > 0
                || (name_of(bs) == "redstone_wire" && wire_power(bs) > 0)
                || (is_solid(bs) && strongly_powered(server, dim, back));
            if input == on(state, "powered") {
                server.redstone.due.lock().unwrap().remove(&(dim, pos));
                return;
            }
            let delay: u64 = prop(state, "delay")
                .and_then(|d| d.parse().ok())
                .unwrap_or(1);
            let mut due = server.redstone.due.lock().unwrap();
            match due.get(&(dim, pos)) {
                Some(&at) if now >= at => {
                    due.remove(&(dim, pos));
                    drop(due);
                    server.set_block(
                        dim,
                        pos,
                        blocks::with_property(state, "powered", &input.to_string()),
                    );
                    let out = add(pos, (-f.0, -f.1, -f.2));
                    super::neighbor_changed(server, dim, out);
                }
                Some(_) => {}
                None => {
                    due.insert((dim, pos), now + delay * 2);
                    server.ticks.schedule(dim, pos, delay * 2);
                }
            }
        }
        _ if n.ends_with("_button") || n.ends_with("pressure_plate") => {
            // 시간이 다 되었으면 놓음
            let pressed = on(state, "powered") || prop(state, "power").is_some_and(|p| p != "0");
            let due = server
                .redstone
                .due
                .lock()
                .unwrap()
                .get(&(dim, pos))
                .copied();
            if pressed && due.is_some_and(|at| now >= at) {
                server.redstone.due.lock().unwrap().remove(&(dim, pos));
                let released = if blocks::has_property(state, "powered") {
                    blocks::with_property(state, "powered", "false")
                } else {
                    blocks::with_property(state, "power", "0")
                };
                server.set_block(dim, pos, released);
                notify_around(server, dim, pos, state);
            }
        }
        _ if n.ends_with("_door") || n.ends_with("_trapdoor") || n.ends_with("fence_gate") => {
            let mut powered = power_at(server, dim, pos) > 0;
            // 문은 두 칸 중 어느 쪽에 신호가 와도
            let other = door_other_half(state, pos);
            if let Some(o) = other {
                powered |= power_at(server, dim, o) > 0;
            }
            if on(state, "powered") != powered {
                let s = blocks::with_property(
                    blocks::with_property(state, "powered", &powered.to_string()),
                    "open",
                    &powered.to_string(),
                );
                server.set_block(dim, pos, s);
                if let Some(o) = other {
                    let os = server.world.get(dim, o.0, o.1, o.2);
                    if name_of(os) == n {
                        let s2 = blocks::with_property(
                            blocks::with_property(os, "powered", &powered.to_string()),
                            "open",
                            &powered.to_string(),
                        );
                        server.set_block(dim, o, s2);
                    }
                }
                server.play_sound(dim, pos, door_sound(n, powered));
            }
        }
        _ => {}
    }
}

/// 가루가 이 블록 쪽으로 신호를 주는가 (블록 위의 가루, 블록을 가리키는 가루).
fn power_into_block(server: &Server, dim: Dimension, block: BlockPos, except: BlockPos) -> bool {
    let above = add(block, (0, 1, 0));
    if above != except && wire_power(server.world.get(dim, above.0, above.1, above.2)) > 0 {
        return true;
    }
    for (d, _) in HORIZONTAL {
        let n = add(block, d);
        if n == except {
            continue;
        }
        let s = server.world.get(dim, n.0, n.1, n.2);
        if emits(s, (-d.0, -d.1, -d.2)) > 0
            && name_of(s) != "redstone_torch"
            && name_of(s) != "redstone_wall_torch"
        {
            return true;
        }
    }
    false
}

fn door_other_half(state: u32, pos: BlockPos) -> Option<BlockPos> {
    match prop(state, "half") {
        Some("lower") if name_of(state).ends_with("_door") => Some(add(pos, (0, 1, 0))),
        Some("upper") if name_of(state).ends_with("_door") => Some(add(pos, (0, -1, 0))),
        _ => None,
    }
}

fn door_sound(n: &str, open: bool) -> &'static str {
    let metal = n.starts_with("iron") || n.contains("copper");
    match (
        n.ends_with("fence_gate"),
        n.ends_with("_trapdoor"),
        metal,
        open,
    ) {
        (true, _, _, true) => "block.fence_gate.open",
        (true, _, _, false) => "block.fence_gate.close",
        (_, true, true, true) => "block.iron_trapdoor.open",
        (_, true, true, false) => "block.iron_trapdoor.close",
        (_, true, false, true) => "block.wooden_trapdoor.open",
        (_, true, false, false) => "block.wooden_trapdoor.close",
        (_, _, true, true) => "block.iron_door.open",
        (_, _, true, false) => "block.iron_door.close",
        (_, _, false, true) => "block.wooden_door.open",
        (_, _, false, false) => "block.wooden_door.close",
    }
}

/// 가루 그물을 다시 계산합니다: 이어진 가루를 모으고, 전원에서 한 칸마다 1씩 줄여 퍼뜨림.
fn update_wires(server: &Server, dim: Dimension, start: BlockPos) {
    let get = |p: BlockPos| server.world.get(dim, p.0, p.1, p.2);
    // 1. 이어진 가루 모으기 (옆, 한 칸 위·아래 계단)
    let mut net: Vec<BlockPos> = Vec::new();
    let mut seen: HashSet<BlockPos> = HashSet::new();
    let mut q = VecDeque::from([start]);
    seen.insert(start);
    while let Some(p) = q.pop_front() {
        if net.len() > 4096 {
            break;
        }
        net.push(p);
        for n in wire_links(server, dim, p) {
            if seen.insert(n) {
                q.push_back(n);
            }
        }
    }
    // 2. 각 가루가 직접 받는 신호
    let mut power: HashMap<BlockPos, u8> = HashMap::new();
    let mut heap: Vec<(u8, BlockPos)> = Vec::new();
    for &p in &net {
        let mut src = 0;
        for d in NEIGHBORS {
            let n = add(p, d);
            let s = get(n);
            if name_of(s) == "redstone_wire" {
                continue;
            }
            src = src.max(emits(s, (-d.0, -d.1, -d.2)));
            if is_solid(s) && strongly_powered(server, dim, n) {
                src = 15;
            }
        }
        power.insert(p, src);
        if src > 0 {
            heap.push((src, p));
        }
    }
    // 3. 퍼뜨리기 (센 것부터)
    let mut level: HashMap<BlockPos, u8> = net.iter().map(|p| (*p, 0)).collect();
    heap.sort();
    while let Some((pw, p)) = heap.pop() {
        if level[&p] >= pw {
            continue;
        }
        level.insert(p, pw);
        if pw <= 1 {
            continue;
        }
        for n in wire_links(server, dim, p) {
            if level.get(&n).is_some_and(|&l| l < pw - 1) {
                heap.push((pw - 1, n));
                heap.sort();
            }
        }
    }
    // 4. 상태 반영 (세기·모양), 바뀐 것만
    for &p in &net {
        let s = get(p);
        let mut ns = blocks::with_property(s, "power", &level[&p].to_string());
        for (d, side) in HORIZONTAL {
            let shape = wire_shape(server, dim, p, d);
            ns = blocks::with_property(ns, side, shape);
        }
        if ns != s {
            server.set_block_quiet(dim, p, ns);
            // 받는 쪽들에 알림 (가루끼리는 이미 계산했으니 가루 아닌 것만)
            for d in NEIGHBORS {
                let n = add(p, d);
                let t = get(n);
                let name = name_of(t);
                if name != "redstone_wire" && is_component(name) {
                    server.ticks.schedule(dim, n, 1);
                }
                if is_solid(t) {
                    for d2 in NEIGHBORS {
                        let m = add(n, d2);
                        let mn = name_of(get(m));
                        if mn != "redstone_wire" && is_component(mn) {
                            server.ticks.schedule(dim, m, 1);
                        }
                    }
                }
            }
        }
    }
}

/// 가루와 이어진 가루들 (옆, 블록을 타고 오르는 계단, 내려가는 계단).
fn wire_links(server: &Server, dim: Dimension, p: BlockPos) -> Vec<BlockPos> {
    let get = |q: BlockPos| server.world.get(dim, q.0, q.1, q.2);
    let mut out = Vec::new();
    let above_solid = is_solid(get(add(p, (0, 1, 0))));
    for (d, _) in HORIZONTAL {
        let n = add(p, d);
        let s = get(n);
        if name_of(s) == "redstone_wire" {
            out.push(n);
        } else if is_solid(s) {
            let up = add(n, (0, 1, 0));
            if !above_solid && name_of(get(up)) == "redstone_wire" {
                out.push(up);
            }
        } else {
            let down = add(n, (0, -1, 0));
            if name_of(get(down)) == "redstone_wire" {
                out.push(down);
            }
        }
    }
    out
}

fn wire_shape(server: &Server, dim: Dimension, p: BlockPos, d: (i32, i32, i32)) -> &'static str {
    let get = |q: BlockPos| server.world.get(dim, q.0, q.1, q.2);
    let n = add(p, d);
    let s = get(n);
    let name = name_of(s);
    if is_solid(s) {
        if name_of(get(add(n, (0, 1, 0)))) == "redstone_wire" && !is_solid(get(add(p, (0, 1, 0)))) {
            return "up";
        }
        return "none";
    }
    if name == "redstone_wire" || name_of(get(add(n, (0, -1, 0)))) == "redstone_wire" {
        return "side";
    }
    if is_component(name)
        && name != "redstone_lamp"
        && !name.ends_with("_door")
        && !name.ends_with("fence_gate")
    {
        if name == "repeater" {
            // 중계기는 앞뒤로만 이어짐
            let f = dir_vec(prop(s, "facing").unwrap_or("north"));
            return if f.0.abs() == d.0.abs() && f.2.abs() == d.2.abs() {
                "side"
            } else {
                "none"
            };
        }
        return "side";
    }
    "none"
}

fn notify_around(server: &Server, dim: Dimension, pos: BlockPos, state: u32) {
    super::neighbor_changed(server, dim, pos);
    if name_of(state) == "lever" || name_of(state).ends_with("_button") {
        super::neighbor_changed(server, dim, add(pos, attached_dir(state)));
    } else {
        super::neighbor_changed(server, dim, add(pos, (0, -1, 0)));
    }
}

/// 손으로 누름: 레버·버튼·나무 문·다락문·울타리 문. 다룬 블록이면 true.
pub fn interact(server: &Server, dim: Dimension, pos: BlockPos, state: u32) -> bool {
    let n = name_of(state);
    if n == "lever" {
        let powered = !on(state, "powered");
        server.set_block(
            dim,
            pos,
            blocks::with_property(state, "powered", &powered.to_string()),
        );
        server.play_sound(dim, pos, "block.lever.click");
        notify_around(server, dim, pos, state);
        return true;
    }
    if n.ends_with("_button") {
        if on(state, "powered") {
            return true;
        }
        server.set_block(dim, pos, blocks::with_property(state, "powered", "true"));
        let wooden = !n.starts_with("stone") && !n.starts_with("polished_blackstone");
        let hold = if wooden { 30 } else { 20 };
        server
            .redstone
            .due
            .lock()
            .unwrap()
            .insert((dim, pos), server.ticks.now() + hold);
        server.ticks.schedule(dim, pos, hold);
        server.play_sound(
            dim,
            pos,
            if wooden {
                "block.wooden_button.click_on"
            } else {
                "block.stone_button.click_on"
            },
        );
        notify_around(server, dim, pos, state);
        return true;
    }
    let metal = n.starts_with("iron_");
    if (n.ends_with("_door") || n.ends_with("_trapdoor") || n.ends_with("fence_gate")) && !metal {
        let open = !on(state, "open");
        server.set_block(
            dim,
            pos,
            blocks::with_property(state, "open", &open.to_string()),
        );
        if let Some(o) = door_other_half(state, pos) {
            let os = server.world.get(dim, o.0, o.1, o.2);
            if name_of(os) == n {
                server.set_block(dim, o, blocks::with_property(os, "open", &open.to_string()));
            }
        }
        server.play_sound(dim, pos, door_sound(n, open));
        return true;
    }
    false
}

/// 압력판을 밟음 (플레이어·몹). 밟는 동안 계속 켜 두고, 내려오면 잠시 뒤 꺼짐.
pub fn stepped_on(server: &Server, dim: Dimension, pos: BlockPos) {
    let state = server.world.get(dim, pos.0, pos.1, pos.2);
    let n = name_of(state);
    if !n.ends_with("pressure_plate") {
        return;
    }
    let now = server.ticks.now();
    server
        .redstone
        .due
        .lock()
        .unwrap()
        .insert((dim, pos), now + 20);
    server.ticks.schedule(dim, pos, 20);
    let pressed = on(state, "powered") || prop(state, "power").is_some_and(|p| p != "0");
    if !pressed {
        let s = if blocks::has_property(state, "powered") {
            blocks::with_property(state, "powered", "true")
        } else {
            blocks::with_property(state, "power", "15")
        };
        server.set_block(dim, pos, s);
        server.play_sound(dim, pos, "block.stone_pressure_plate.click_on");
        notify_around(server, dim, pos, state);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::packets::status::ServerInfo;
    use crate::world::blockset::{b, bp};

    fn run(s: &Server, ticks: u64) {
        for _ in 0..ticks {
            let now = s.ticks.now() + 1;
            super::super::tick(s, now, &[]);
        }
    }

    #[test]
    fn lever_lights_lamp_through_wire_and_wire_fades() {
        let s = Server::new(
            ServerInfo {
                name: "t".into(),
                description: "t".into(),
                max_players: 1,
            },
            3,
        );
        let d = Dimension::Overworld;
        let y = 200;
        for x in -2..20 {
            s.world.set(d, x, y - 1, 0, block_stone());
            s.world.set(d, x, y, 0, crate::data::block::AIR);
        }
        // 레버(바닥) — 가루 16칸 — 램프
        s.set_block(
            d,
            (0, y, 0),
            bp("lever", &[("face", "floor"), ("powered", "false")]),
        );
        for x in 1..=16 {
            s.set_block(d, (x, y, 0), b("redstone_wire"));
        }
        s.set_block(d, (17, y, 0), bp("redstone_lamp", &[("lit", "false")]));
        run(&s, 5);
        assert!(interact(&s, d, (0, y, 0), s.world.get(d, 0, y, 0)));
        run(&s, 10);
        assert_eq!(wire_power(s.world.get(d, 1, y, 0)), 15);
        assert_eq!(wire_power(s.world.get(d, 15, y, 0)), 1);
        assert_eq!(wire_power(s.world.get(d, 16, y, 0)), 0, "16th dust is dark");
        assert!(!on(s.world.get(d, 17, y, 0), "lit"), "too far");
        // 램프를 가깝게
        s.set_block(d, (5, y, 0), bp("redstone_lamp", &[("lit", "false")]));
        run(&s, 10);
        assert!(on(s.world.get(d, 5, y, 0), "lit"));
        // 끄면 꺼짐
        interact(&s, d, (0, y, 0), s.world.get(d, 0, y, 0));
        run(&s, 10);
        assert!(!on(s.world.get(d, 5, y, 0), "lit"));
        assert_eq!(wire_power(s.world.get(d, 1, y, 0)), 0);
    }

    fn block_stone() -> u32 {
        crate::data::block::STONE
    }
}
