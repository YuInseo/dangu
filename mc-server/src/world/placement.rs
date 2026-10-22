//! 블록을 놓을 때 상태(방향, 위/아래 반쪽 등)를 정하기. 바닐라의 블록별 규칙을 모두
//! 옮기지는 않고, 속성 이름으로 흔한 경우를 처리합니다.

use crate::data::block;
use crate::data::blocks::{self, BlockId, StateId};
use crate::packets::world::{BlockPos, Face, add};

/// 놓을 때 알아야 하는 것.
pub struct Context {
    /// 누른 면.
    pub face: Face,
    /// 누른 자리의 블록 안 높이 (0..1).
    pub cursor_y: f32,
    pub yaw: f32,
    pub pitch: f32,
    /// 놓일 자리에 원래 있던 블록 (물이면 물에 잠긴 상태로).
    pub replacing: StateId,
}

/// 플레이어 쪽을 보는 대신 플레이어와 같은 쪽을 보는 블록들 (계단, 문, 울타리 문, 침대 등).
fn faces_away_from_player(name: &str) -> bool {
    name.ends_with("_stairs")
        || (name.ends_with("_door") && !name.ends_with("trapdoor"))
        || name.ends_with("_fence_gate")
        || name.ends_with("_bed")
        || name == "repeater"
        || name == "comparator"
}

/// 바라보는 방향을 그대로 따르는 블록 (관측기 등). 나머지 6방향 블록은 플레이어를 봅니다.
fn faces_look_direction(name: &str) -> bool {
    name == "observer"
}

/// 누른 면에 붙는 블록 (사다리 등): facing = 누른 면.
fn attaches_to_face(name: &str) -> bool {
    name == "ladder" || name.ends_with("_wall_sign") || name.ends_with("_wall_banner")
}

pub fn state_for(block: BlockId, ctx: &Context) -> StateId {
    let name = blocks::name(block);
    let mut s = blocks::default_state(block);
    let player_dir = Face::from_yaw(ctx.yaw);

    if blocks::has_property(s, "axis") {
        s = blocks::with_property(s, "axis", ctx.face.axis());
    }

    if let Some(facing_values) = facing_values(s) {
        let horizontal_only = facing_values == 4;
        let facing = if attaches_to_face(name) && ctx.face.is_horizontal() {
            ctx.face
        } else if horizontal_only {
            if faces_away_from_player(name) {
                player_dir
            } else {
                player_dir.opposite()
            }
        } else {
            let look = Face::from_look(ctx.yaw, ctx.pitch);
            if faces_look_direction(name) {
                look
            } else {
                look.opposite()
            }
        };
        s = blocks::with_property(s, "facing", facing.name());
    }

    // 버튼·레버: 바닥/벽/천장
    if blocks::has_property(s, "face") {
        let (attach, facing) = match ctx.face {
            Face::Up => ("floor", player_dir),
            Face::Down => ("ceiling", player_dir),
            f => ("wall", f),
        };
        s = blocks::with_property(s, "face", attach);
        s = blocks::with_property(s, "facing", facing.name());
    }

    // 위쪽 반에 놓기: 아랫면을 눌렀거나, 옆면의 위쪽 절반을 눌렀을 때
    let upper = ctx.face == Face::Down || (ctx.face.is_horizontal() && ctx.cursor_y > 0.5);
    if blocks::property(s, "half").is_some_and(|v| v == "bottom" || v == "top") {
        s = blocks::with_property(s, "half", if upper { "top" } else { "bottom" });
    }
    if blocks::property(s, "type").is_some_and(|v| v == "bottom" || v == "top") {
        s = blocks::with_property(s, "type", if upper { "top" } else { "bottom" });
    }

    if blocks::has_property(s, "waterlogged") && ctx.replacing == block::WATER {
        s = blocks::with_property(s, "waterlogged", "true");
    }
    s
}

fn facing_values(state: StateId) -> Option<usize> {
    blocks::property_values(state, "facing").map(<[_]>::len)
}

/// 두 칸짜리 블록의 나머지 반쪽: 문·큰 꽃의 윗부분, 침대 머리.
pub fn partner(pos: BlockPos, state: StateId) -> Option<(BlockPos, StateId)> {
    match blocks::property(state, "half") {
        Some("lower") => {
            return Some((
                add(pos, (0, 1, 0)),
                blocks::with_property(state, "half", "upper"),
            ));
        }
        Some("upper") => {
            return Some((
                add(pos, (0, -1, 0)),
                blocks::with_property(state, "half", "lower"),
            ));
        }
        _ => {}
    }
    let facing = facing_from_state(state)?;
    match blocks::property(state, "part") {
        Some("foot") => Some((
            add(pos, facing.offset()),
            blocks::with_property(state, "part", "head"),
        )),
        Some("head") => Some((
            add(pos, facing.opposite().offset()),
            blocks::with_property(state, "part", "foot"),
        )),
        _ => None,
    }
}

fn facing_from_state(state: StateId) -> Option<Face> {
    Some(match blocks::property(state, "facing")? {
        "north" => Face::North,
        "south" => Face::South,
        "west" => Face::West,
        "east" => Face::East,
        "up" => Face::Up,
        _ => Face::Down,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx(face: Face, cursor_y: f32, yaw: f32) -> Context {
        Context {
            face,
            cursor_y,
            yaw,
            pitch: 0.0,
            replacing: block::AIR,
        }
    }

    fn place(name: &str, c: &Context) -> StateId {
        state_for(blocks::by_name(name).unwrap(), c)
    }

    #[test]
    fn logs_follow_clicked_face() {
        let s = place("oak_log", &ctx(Face::East, 0.5, 0.0));
        assert_eq!(blocks::property(s, "axis"), Some("x"));
    }

    #[test]
    fn furnace_faces_player_and_stairs_face_away() {
        // 남쪽(yaw 0)을 보고 놓으면 화로는 북쪽(플레이어 쪽)을 봅니다.
        let s = place("furnace", &ctx(Face::Up, 1.0, 0.0));
        assert_eq!(blocks::property(s, "facing"), Some("north"));
        let s = place("oak_stairs", &ctx(Face::Up, 1.0, 0.0));
        assert_eq!(blocks::property(s, "facing"), Some("south"));
        assert_eq!(blocks::property(s, "half"), Some("bottom"));
        let s = place("oak_stairs", &ctx(Face::North, 0.8, 0.0));
        assert_eq!(blocks::property(s, "half"), Some("top"));
    }

    #[test]
    fn slab_top_or_bottom() {
        assert_eq!(
            blocks::property(place("oak_slab", &ctx(Face::Down, 0.0, 0.0)), "type"),
            Some("top")
        );
        assert_eq!(
            blocks::property(place("oak_slab", &ctx(Face::Up, 1.0, 0.0)), "type"),
            Some("bottom")
        );
    }

    #[test]
    fn waterlogs_when_replacing_water() {
        let mut c = ctx(Face::Up, 1.0, 0.0);
        c.replacing = block::WATER;
        assert_eq!(
            blocks::property(place("oak_slab", &c), "waterlogged"),
            Some("true")
        );
    }

    #[test]
    fn doors_and_beds_have_partners() {
        let door = place("oak_door", &ctx(Face::Up, 1.0, 0.0));
        let (above, upper) = partner((0, 64, 0), door).unwrap();
        assert_eq!(above, (0, 65, 0));
        assert_eq!(blocks::property(upper, "half"), Some("upper"));
        assert_eq!(partner(above, upper).unwrap(), ((0, 64, 0), door));

        let bed = place("red_bed", &ctx(Face::Up, 1.0, 0.0));
        assert_eq!(blocks::property(bed, "part"), Some("foot"));
        let (head_pos, head) = partner((0, 64, 0), bed).unwrap();
        assert_eq!(head_pos, (0, 64, 1)); // 남쪽을 보고 놓았으니 머리는 남쪽
        assert_eq!(blocks::property(head, "part"), Some("head"));
    }
}
