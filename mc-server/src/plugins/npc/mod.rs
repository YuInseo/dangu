//! 사용자 정의 NPC (Citizens 같은 플러그인 기능). 채널(인스턴스)마다 `npcs.json`에 남습니다.
//!
//! - 모양: `player`(마네킹 — 플레이어 모습, 탭 목록에 안 뜸, 스킨 지정 가능) 또는 몹 이름
//!   (`villager`, `zombie` …). 머리 위 이름(`&` 색 코드), 마네킹은 이름 아래 설명 한 줄.
//! - 가까운 사람을 쳐다봄(시선), 움직이지 않고 맞지 않음.
//! - 우클릭하면: 메시지들을 보내고, 동작(`lottery` 복권 판매소, `server:<인스턴스>` 이동)을
//!   하고, 명령어들을 실행 (그 사람으로, 또는 `console`로). `{player}`는 누른 사람 이름.
//!
//! 관리는 게임 안 `/npc` (= `/엔피시`).

pub(crate) mod session;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;

use crate::auth::Property;
use crate::game::entities::{Entity, Kind, Vec3, dist2};
use crate::json::{self, Json};
use crate::packets::entity::Meta;
use crate::server::{PlayerState, Server};
use crate::world::Dimension;

/// 쳐다보는 거리.
const LOOK_RANGE: f64 = 8.0;

#[derive(Debug, Clone, PartialEq)]
pub struct NpcCommand {
    /// 콘솔로 실행 (아니면 누른 사람으로).
    pub console: bool,
    pub line: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Npc {
    pub id: u32,
    /// 머리 위 이름 (`&a` 색 코드).
    pub name: String,
    /// `player` 또는 몹 이름.
    pub kind: String,
    /// 마네킹 스킨: (가져온 이름, 프로필 속성).
    pub skin: Option<(String, Vec<Property>)>,
    pub dimension: Dimension,
    pub pos: Vec3,
    pub yaw: f32,
    pub look_close: bool,
    /// 이름 아래 한 줄 (마네킹만).
    pub description: Option<String>,
    pub messages: Vec<String>,
    pub commands: Vec<NpcCommand>,
    /// 기본 동작: `lottery`, `server:<이름>`.
    pub action: Option<String>,
}

impl Npc {
    pub fn new(id: u32, name: &str, kind: &str, dimension: Dimension, pos: Vec3, yaw: f32) -> Self {
        Self {
            id,
            name: name.to_owned(),
            kind: kind.to_owned(),
            skin: None,
            dimension,
            pos,
            yaw,
            look_close: true,
            description: None,
            messages: Vec::new(),
            commands: Vec::new(),
            action: None,
        }
    }

    pub fn is_player(&self) -> bool {
        self.kind == "player" || self.kind == "mannequin"
    }

    fn type_id(&self) -> Option<i32> {
        crate::data::entity_id(if self.is_player() {
            "mannequin"
        } else {
            &self.kind
        })
    }

    /// 띄울 때 보낼 모습.
    pub fn meta(&self) -> Vec<Meta> {
        let mut m = vec![Meta::Name(2, self.name.clone()), Meta::Bool(3, true)];
        if self.is_player() {
            let props = self.skin.as_ref().map(|s| s.1.clone()).unwrap_or_default();
            m.push(Meta::Arm(15, 1));
            m.push(Meta::Byte(16, 0x7F)); // 겉옷 레이어 모두
            m.push(Meta::Profile(
                17,
                self.uuid(),
                format!("npc{}", self.id),
                props,
            ));
            m.push(Meta::Bool(18, true)); // 밀리지 않음
            m.push(Meta::OptText(19, self.description.clone()));
        } else {
            m.push(Meta::Byte(15, 0x01)); // AI 없음
        }
        m
    }

    /// 마네킹 프로필의 UUID (NPC 번호에서 만든 v4 모양).
    fn uuid(&self) -> u128 {
        0x4e50_4300_0000_4000_8000_0000_0000_0000 | self.id as u128
    }
}

struct Inner {
    defs: Vec<Npc>,
    /// NPC 번호 → 띄운 엔티티.
    spawned: HashMap<u32, i32>,
}

pub struct Npcs {
    inner: Mutex<Inner>,
    path: Option<PathBuf>,
}

impl Npcs {
    pub fn open(path: Option<PathBuf>) -> Self {
        let defs = path
            .as_ref()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|t| json::parse(&t).ok())
            .map(|j| {
                j.get("npcs")
                    .and_then(Json::as_array)
                    .unwrap_or(&[])
                    .iter()
                    .filter_map(from_json)
                    .collect()
            })
            .unwrap_or_default();
        Self {
            inner: Mutex::new(Inner {
                defs,
                spawned: HashMap::new(),
            }),
            path,
        }
    }

    fn save(&self, defs: &[Npc]) {
        if let Some(p) = &self.path {
            let j = Json::Object(vec![(
                "npcs".into(),
                Json::Array(defs.iter().map(to_json).collect()),
            )]);
            let tmp = p.with_extension("json.tmp");
            if std::fs::write(&tmp, j.pretty()).is_ok() {
                let _ = std::fs::rename(tmp, p);
            }
        }
    }

    pub fn all(&self) -> Vec<Npc> {
        self.inner.lock().unwrap().defs.clone()
    }

    pub fn get(&self, id: u32) -> Option<Npc> {
        self.inner
            .lock()
            .unwrap()
            .defs
            .iter()
            .find(|n| n.id == id)
            .cloned()
    }

    /// 만들고 번호를 돌려줍니다.
    pub fn create(&self, server: &Server, mut npc: Npc) -> u32 {
        let id = {
            let mut inner = self.inner.lock().unwrap();
            npc.id = inner.defs.iter().map(|n| n.id).max().unwrap_or(0) + 1;
            inner.defs.push(npc.clone());
            self.save(&inner.defs);
            npc.id
        };
        self.respawn(server, id);
        id
    }

    /// 고치고 다시 띄웁니다. 없으면 false.
    pub fn edit(&self, server: &Server, id: u32, f: impl FnOnce(&mut Npc)) -> bool {
        {
            let mut inner = self.inner.lock().unwrap();
            let Some(n) = inner.defs.iter_mut().find(|n| n.id == id) else {
                return false;
            };
            f(n);
            self.save(&inner.defs);
        }
        self.respawn(server, id);
        true
    }

    pub fn remove(&self, server: &Server, id: u32) -> bool {
        let entity = {
            let mut inner = self.inner.lock().unwrap();
            let before = inner.defs.len();
            inner.defs.retain(|n| n.id != id);
            if inner.defs.len() == before {
                return false;
            }
            self.save(&inner.defs);
            inner.spawned.remove(&id)
        };
        if let Some(e) = entity {
            server.entities.remove(server, e);
        }
        true
    }

    /// 이 엔티티가 NPC면 그 번호.
    pub fn by_entity(&self, entity: i32) -> Option<u32> {
        self.inner
            .lock()
            .unwrap()
            .spawned
            .iter()
            .find(|(_, e)| **e == entity)
            .map(|(n, _)| *n)
    }

    pub fn entity_of(&self, id: u32) -> Option<i32> {
        self.inner.lock().unwrap().spawned.get(&id).copied()
    }

    /// 이 동작을 하는 NPC가 없으면 만듭니다 (복권 판매소 등). 그 NPC 번호.
    pub fn ensure_action(&self, server: &Server, action: &str, make: impl FnOnce() -> Npc) -> u32 {
        let found = self
            .inner
            .lock()
            .unwrap()
            .defs
            .iter()
            .find(|n| n.action.as_deref() == Some(action))
            .map(|n| n.id);
        match found {
            Some(id) => id,
            None => {
                let mut n = make();
                n.action = Some(action.to_owned());
                self.create(server, n)
            }
        }
    }

    /// 엔티티를 (다시) 띄웁니다.
    pub fn respawn(&self, server: &Server, id: u32) {
        let (def, old) = {
            let mut inner = self.inner.lock().unwrap();
            let def = inner.defs.iter().find(|n| n.id == id).cloned();
            (def, inner.spawned.remove(&id))
        };
        if let Some(e) = old {
            server.entities.remove(server, e);
        }
        let Some(def) = def else { return };
        let Some(type_id) = def.type_id() else { return };
        let entity = server.allocate_entity_id();
        self.inner.lock().unwrap().spawned.insert(id, entity);
        server.entities.insert(
            server,
            Entity {
                id: entity,
                uuid: u128::from_le_bytes(crate::crypto::random::bytes::<16>()),
                dimension: def.dimension,
                pos: def.pos,
                vel: (0.0, 0.0, 0.0),
                yaw: def.yaw,
                pitch: 0.0,
                on_ground: true,
                collided: false,
                age: 0,
                kind: Kind::Npc {
                    npc: id,
                    type_id,
                    meta: def.meta(),
                    look_close: def.look_close,
                },
            },
        );
    }

    /// 없어진 엔티티를 다시 띄웁니다 (켤 때, 지워졌을 때).
    pub fn ensure_spawned(&self, server: &Server) {
        let missing: Vec<u32> = {
            let inner = self.inner.lock().unwrap();
            inner
                .defs
                .iter()
                .filter(|n| {
                    inner
                        .spawned
                        .get(&n.id)
                        .is_none_or(|e| server.entities.get(*e).is_none())
                })
                .map(|n| n.id)
                .collect()
        };
        for id in missing {
            self.respawn(server, id);
        }
    }

    /// 가장 가까운 NPC (`within` 칸 안).
    pub fn nearest(&self, dim: Dimension, at: Vec3, within: f64) -> Option<u32> {
        self.inner
            .lock()
            .unwrap()
            .defs
            .iter()
            .filter(|n| n.dimension == dim && dist2(n.pos, at) <= within * within)
            .min_by(|a, b| dist2(a.pos, at).total_cmp(&dist2(b.pos, at)))
            .map(|n| n.id)
    }
}

/// 엔티티 틱: 가까운 사람을 쳐다봅니다. 고개가 바뀌었으면 true.
pub fn tick(e: &mut Entity, players: &[PlayerState]) -> bool {
    let Kind::Npc {
        look_close: true, ..
    } = e.kind
    else {
        return false;
    };
    let Some(p) = players
        .iter()
        .filter(|p| p.dimension == e.dimension && !p.dead)
        .filter(|p| dist2(p.position, e.pos) < LOOK_RANGE * LOOK_RANGE)
        .min_by(|a, b| dist2(a.position, e.pos).total_cmp(&dist2(b.position, e.pos)))
    else {
        return false;
    };
    let (dx, dy, dz) = (
        p.position.0 - e.pos.0,
        p.position.1 + 1.62 - (e.pos.1 + 1.62),
        p.position.2 - e.pos.2,
    );
    let yaw = (-dx.atan2(dz)).to_degrees() as f32;
    let pitch = (-dy.atan2((dx * dx + dz * dz).sqrt())).to_degrees() as f32;
    if (yaw - e.yaw).abs() < 2.0 && (pitch - e.pitch).abs() < 2.0 {
        return false;
    }
    e.yaw = yaw;
    e.pitch = pitch;
    true
}

/// 우클릭했을 때 실행할 것: (보낼 메시지들, 동작, 명령어들) — `{player}`를 바꾼 뒤.
pub fn on_click(npc: &Npc, player: &str) -> (Vec<String>, Option<String>, Vec<NpcCommand>) {
    let fill = |s: &str| s.replace("{player}", player);
    (
        npc.messages.iter().map(|m| fill(m)).collect(),
        npc.action.clone(),
        npc.commands
            .iter()
            .map(|c| NpcCommand {
                console: c.console,
                line: fill(&c.line),
            })
            .collect(),
    )
}

fn to_json(n: &Npc) -> Json {
    let s = |v: &str| Json::String(v.to_owned());
    let mut f = vec![
        ("id".to_owned(), Json::Number(n.id as f64)),
        ("name".into(), s(&n.name)),
        ("kind".into(), s(&n.kind)),
        ("dimension".into(), s(n.dimension.name())),
        (
            "pos".into(),
            Json::Array(vec![
                Json::Number(n.pos.0),
                Json::Number(n.pos.1),
                Json::Number(n.pos.2),
            ]),
        ),
        ("yaw".into(), Json::Number(n.yaw as f64)),
        ("look_close".into(), Json::Bool(n.look_close)),
        (
            "messages".into(),
            Json::Array(n.messages.iter().map(|m| s(m)).collect()),
        ),
        (
            "commands".into(),
            Json::Array(
                n.commands
                    .iter()
                    .map(|c| {
                        Json::Object(vec![
                            ("console".into(), Json::Bool(c.console)),
                            ("line".into(), s(&c.line)),
                        ])
                    })
                    .collect(),
            ),
        ),
    ];
    if let Some(d) = &n.description {
        f.push(("description".into(), s(d)));
    }
    if let Some(a) = &n.action {
        f.push(("action".into(), s(a)));
    }
    if let Some((from, props)) = &n.skin {
        f.push((
            "skin".into(),
            Json::Object(vec![
                ("from".into(), s(from)),
                (
                    "properties".into(),
                    Json::Array(
                        props
                            .iter()
                            .map(|p| {
                                let mut o = vec![
                                    ("name".to_owned(), s(&p.name)),
                                    ("value".into(), s(&p.value)),
                                ];
                                if let Some(sig) = &p.signature {
                                    o.push(("signature".into(), s(sig)));
                                }
                                Json::Object(o)
                            })
                            .collect(),
                    ),
                ),
            ]),
        ));
    }
    Json::Object(f)
}

fn from_json(j: &Json) -> Option<Npc> {
    let st = |k: &str| j.get(k).and_then(Json::as_str).map(str::to_owned);
    let pos = j.get("pos")?.as_array()?;
    let p = |i: usize| pos.get(i).and_then(Json::as_f64);
    let dimension = st("dimension")
        .and_then(|d| Dimension::from_name(&d))
        .unwrap_or(Dimension::Overworld);
    let skin = j.get("skin").map(|s| {
        (
            s.get("from")
                .and_then(Json::as_str)
                .unwrap_or("")
                .to_owned(),
            s.get("properties")
                .and_then(Json::as_array)
                .unwrap_or(&[])
                .iter()
                .filter_map(|p| {
                    Some(Property {
                        name: p.get("name")?.as_str()?.to_owned(),
                        value: p.get("value")?.as_str()?.to_owned(),
                        signature: p.get("signature").and_then(Json::as_str).map(str::to_owned),
                    })
                })
                .collect(),
        )
    });
    Some(Npc {
        id: j.get("id")?.as_f64()? as u32,
        name: st("name")?,
        kind: st("kind").unwrap_or_else(|| "player".into()),
        skin,
        dimension,
        pos: (p(0)?, p(1)?, p(2)?),
        yaw: j.get("yaw").and_then(Json::as_f64).unwrap_or(0.0) as f32,
        look_close: j.get("look_close").and_then(Json::as_bool).unwrap_or(true),
        description: st("description"),
        messages: j
            .get("messages")
            .and_then(Json::as_array)
            .unwrap_or(&[])
            .iter()
            .filter_map(|m| m.as_str().map(str::to_owned))
            .collect(),
        commands: j
            .get("commands")
            .and_then(Json::as_array)
            .unwrap_or(&[])
            .iter()
            .filter_map(|c| {
                Some(NpcCommand {
                    console: c.get("console").and_then(Json::as_bool).unwrap_or(false),
                    line: c.get("line")?.as_str()?.to_owned(),
                })
            })
            .collect(),
        action: st("action"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn npcs_round_trip_through_json() {
        let mut n = Npc::new(
            3,
            "&a상인 &l철수",
            "player",
            Dimension::Nether,
            (1.5, 64.0, -2.5),
            90.0,
        );
        n.skin = Some((
            "Notch".into(),
            vec![Property {
                name: "textures".into(),
                value: "abc".into(),
                signature: Some("sig".into()),
            }],
        ));
        n.description = Some("&7우클릭".into());
        n.messages.push("안녕하세요 {player}님".into());
        n.commands.push(NpcCommand {
            console: true,
            line: "say {player} 방문".into(),
        });
        n.action = Some("server:rpg".into());
        assert_eq!(from_json(&to_json(&n)), Some(n.clone()));
        let (msgs, action, cmds) = on_click(&n, "Alex");
        assert_eq!(msgs, vec!["안녕하세요 Alex님"]);
        assert_eq!(action.as_deref(), Some("server:rpg"));
        assert_eq!(cmds[0].line, "say Alex 방문");
        assert!(n.type_id().is_some(), "mannequin entity exists");
        assert!(
            Npc::new(
                1,
                "x",
                "villager",
                Dimension::Overworld,
                (0.0, 0.0, 0.0),
                0.0
            )
            .type_id()
            .is_some()
        );
    }
}
