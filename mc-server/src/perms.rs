//! 권한: 관리자(OP) 단계, 권한 노드와 그룹, 밴, 화이트리스트.
//!
//! - **OP 단계** 0~4 (바닐라와 같음): 0 일반, 1 스폰 보호 무시, 2 치트 명령어(/gamemode,
//!   /give, /tp…), 3 관리(/kick, /ban, /op…), 4 서버(/stop).
//! - **권한 노드** (Bukkit식 `dangu.command.give`): 사람마다, 그룹마다 줄 수 있고 `*`·`a.b.*`
//!   와일드카드와 `-노드`(빼앗기)를 씁니다. 사람에게 준 것이 그룹 것보다, 정확한 노드가
//!   와일드카드보다, 빼앗기가 주기보다 먼저입니다. 아무 규칙도 없으면 명령어의 기본 OP
//!   단계로 정합니다.
//! - 모든 사람은 `default` 그룹에 속합니다.
//!
//! 파일 (월드 폴더, 스피곳과 비슷한 이름): `ops.json`, `permissions.json`,
//! `banned-players.json`, `banned-ips.json`, `whitelist.json`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use crate::json::{self, Json};
use crate::uuid::format_uuid;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Op {
    pub uuid: u128,
    pub name: String,
    pub level: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Group {
    pub nodes: Vec<String>,
    pub inherits: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct User {
    pub name: String,
    pub groups: Vec<String>,
    pub nodes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ban {
    pub uuid: u128,
    pub name: String,
    pub reason: String,
    pub by: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IpBan {
    pub ip: String,
    pub reason: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Data {
    pub ops: Vec<Op>,
    pub groups: BTreeMap<String, Group>,
    pub users: BTreeMap<u128, User>,
    pub bans: Vec<Ban>,
    pub ip_bans: Vec<IpBan>,
    pub whitelist_enabled: bool,
    pub whitelist: Vec<(u128, String)>,
}

pub struct Permissions {
    dir: Option<PathBuf>,
    /// 모두를 OP 4로 (혼자 쓰는 개발 서버·테스트용).
    pub everyone_op: bool,
    data: Mutex<Data>,
}

/// 노드 하나가 규칙 하나에 얼마나 맞는가: 높을수록 구체적. None = 안 맞음.
fn specificity(rule: &str, node: &str) -> Option<usize> {
    if rule == node {
        Some(usize::MAX)
    } else if rule == "*" {
        Some(0)
    } else if let Some(prefix) = rule.strip_suffix(".*") {
        (node.starts_with(prefix) && node[prefix.len()..].starts_with('.'))
            .then(|| prefix.len() + 1)
    } else {
        None
    }
}

/// 규칙 목록에서 가장 구체적인 판정. 같은 구체성이면 빼앗기가 이깁니다.
fn decide(rules: &[String], node: &str) -> Option<(usize, bool)> {
    let mut best: Option<(usize, bool)> = None;
    for r in rules {
        let (negated, rule) = match r.strip_prefix('-') {
            Some(rest) => (true, rest),
            None => (false, r.as_str()),
        };
        if let Some(s) = specificity(rule, node) {
            let allow = !negated;
            best = match best {
                Some((bs, ba)) if bs > s || (bs == s && !ba) => Some((bs, ba)),
                _ => Some((s, allow)),
            };
        }
    }
    best
}

impl Data {
    pub fn op_level(&self, uuid: u128) -> u8 {
        self.ops
            .iter()
            .find(|o| o.uuid == uuid)
            .map_or(0, |o| o.level)
    }

    /// 그룹의 규칙 (상속 포함, 순환은 끊음).
    fn group_rules(&self, name: &str, seen: &mut Vec<String>, out: &mut Vec<String>) {
        if seen.iter().any(|s| s == name) {
            return;
        }
        seen.push(name.to_owned());
        if let Some(g) = self.groups.get(name) {
            out.extend(g.nodes.iter().cloned());
            for parent in &g.inherits {
                self.group_rules(parent, seen, out);
            }
        }
    }

    /// 권한 노드 판정. 규칙이 없으면 None.
    pub fn check(&self, uuid: u128, node: &str) -> Option<bool> {
        if let Some(u) = self.users.get(&uuid)
            && let Some((_, allow)) = decide(&u.nodes, node)
        {
            return Some(allow);
        }
        let mut rules = Vec::new();
        let mut seen = Vec::new();
        if let Some(u) = self.users.get(&uuid) {
            for g in &u.groups {
                self.group_rules(g, &mut seen, &mut rules);
            }
        }
        self.group_rules("default", &mut seen, &mut rules);
        decide(&rules, node).map(|(_, allow)| allow)
    }

    pub fn banned(&self, uuid: u128) -> Option<&Ban> {
        self.bans.iter().find(|b| b.uuid == uuid)
    }

    pub fn ip_banned(&self, ip: &str) -> Option<&IpBan> {
        self.ip_bans.iter().find(|b| b.ip == ip)
    }

    pub fn whitelisted(&self, uuid: u128) -> bool {
        self.whitelist.iter().any(|(u, _)| *u == uuid)
    }

    pub fn user_mut(&mut self, uuid: u128, name: &str) -> &mut User {
        let u = self.users.entry(uuid).or_default();
        u.name = name.to_owned();
        u
    }

    /// 이름으로 저장된 사람 찾기 (OP·권한·밴·화이트리스트 어디든).
    pub fn find_name(&self, name: &str) -> Option<(u128, String)> {
        let eq = |n: &str| n.eq_ignore_ascii_case(name);
        self.ops
            .iter()
            .find(|o| eq(&o.name))
            .map(|o| (o.uuid, o.name.clone()))
            .or_else(|| {
                self.users
                    .iter()
                    .find(|(_, u)| eq(&u.name))
                    .map(|(id, u)| (*id, u.name.clone()))
            })
            .or_else(|| {
                self.bans
                    .iter()
                    .find(|b| eq(&b.name))
                    .map(|b| (b.uuid, b.name.clone()))
            })
            .or_else(|| self.whitelist.iter().find(|(_, n)| eq(n)).cloned())
    }
}

impl Permissions {
    pub fn open(dir: Option<PathBuf>, everyone_op: bool) -> Result<Self, String> {
        let data = match &dir {
            Some(d) => load_all(d)?,
            None => Data::default(),
        };
        Ok(Self {
            dir,
            everyone_op,
            data: Mutex::new(data),
        })
    }

    pub fn op_level(&self, uuid: u128) -> u8 {
        if self.everyone_op {
            4
        } else {
            self.data.lock().unwrap().op_level(uuid)
        }
    }

    /// `node` 권한이 있는가. 규칙이 없으면 OP 단계가 `default_level` 이상인지.
    pub fn has(&self, uuid: u128, node: &str, default_level: u8) -> bool {
        let level = self.op_level(uuid);
        match self.data.lock().unwrap().check(uuid, node) {
            Some(allow) => allow,
            None => level >= default_level,
        }
    }

    pub fn read<T>(&self, f: impl FnOnce(&Data) -> T) -> T {
        f(&self.data.lock().unwrap())
    }

    /// 바꾸고 저장합니다.
    pub fn update<T>(&self, f: impl FnOnce(&mut Data) -> T) -> T {
        let mut d = self.data.lock().unwrap();
        let r = f(&mut d);
        if let Some(dir) = &self.dir
            && let Err(e) = save_all(dir, &d)
        {
            eprintln!("권한 저장 실패: {e}");
        }
        r
    }
}

// ---- 파일 ----

fn uuid_str(u: u128) -> Json {
    Json::String(format_uuid(u))
}

fn parse_uuid(v: Option<&Json>) -> Option<u128> {
    u128::from_str_radix(&v?.as_str()?.replace('-', ""), 16).ok()
}

fn strings(v: Option<&Json>) -> Vec<String> {
    v.and_then(Json::as_array)
        .unwrap_or_default()
        .iter()
        .filter_map(|s| s.as_str().map(str::to_owned))
        .collect()
}

fn str_array(v: &[String]) -> Json {
    Json::Array(v.iter().cloned().map(Json::String).collect())
}

fn read_json(dir: &Path, name: &str) -> Result<Option<Json>, String> {
    match std::fs::read_to_string(dir.join(name)) {
        Ok(t) => json::parse(&t)
            .map(Some)
            .map_err(|e| format!("{name}: {e}")),
        Err(_) => Ok(None),
    }
}

fn load_all(dir: &Path) -> Result<Data, String> {
    let mut d = Data::default();
    if let Some(Json::Array(ops)) = read_json(dir, "ops.json")? {
        for o in &ops {
            if let Some(uuid) = parse_uuid(o.get("uuid")) {
                let level = match o.get("level") {
                    Some(Json::Number(n)) => (*n as u8).clamp(1, 4),
                    _ => 4,
                };
                d.ops.push(Op {
                    uuid,
                    name: o.get("name").and_then(Json::as_str).unwrap_or("").into(),
                    level,
                });
            }
        }
    }
    if let Some(p) = read_json(dir, "permissions.json")? {
        if let Some(Json::Object(groups)) = p.get("groups") {
            for (name, g) in groups {
                d.groups.insert(
                    name.clone(),
                    Group {
                        nodes: strings(g.get("permissions")),
                        inherits: strings(g.get("inherits")),
                    },
                );
            }
        }
        if let Some(Json::Object(users)) = p.get("users") {
            for (id, u) in users {
                if let Ok(uuid) = u128::from_str_radix(&id.replace('-', ""), 16) {
                    d.users.insert(
                        uuid,
                        User {
                            name: u.get("name").and_then(Json::as_str).unwrap_or("").into(),
                            groups: strings(u.get("groups")),
                            nodes: strings(u.get("permissions")),
                        },
                    );
                }
            }
        }
    }
    if let Some(Json::Array(bans)) = read_json(dir, "banned-players.json")? {
        for b in &bans {
            if let Some(uuid) = parse_uuid(b.get("uuid")) {
                let s = |k: &str| b.get(k).and_then(Json::as_str).unwrap_or("").to_owned();
                d.bans.push(Ban {
                    uuid,
                    name: s("name"),
                    reason: s("reason"),
                    by: s("source"),
                });
            }
        }
    }
    if let Some(Json::Array(bans)) = read_json(dir, "banned-ips.json")? {
        for b in &bans {
            if let Some(ip) = b.get("ip").and_then(Json::as_str) {
                d.ip_bans.push(IpBan {
                    ip: ip.into(),
                    reason: b.get("reason").and_then(Json::as_str).unwrap_or("").into(),
                });
            }
        }
    }
    if let Some(w) = read_json(dir, "whitelist.json")? {
        d.whitelist_enabled = matches!(w.get("enabled"), Some(Json::Bool(true)));
        for p in w
            .get("players")
            .and_then(Json::as_array)
            .unwrap_or_default()
        {
            if let Some(uuid) = parse_uuid(p.get("uuid")) {
                d.whitelist.push((
                    uuid,
                    p.get("name").and_then(Json::as_str).unwrap_or("").into(),
                ));
            }
        }
    }
    Ok(d)
}

fn save_all(dir: &Path, d: &Data) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    let write = |name: &str, v: Json| {
        crate::game::playerdata::write_atomically(dir, name, v.pretty().as_bytes())
    };
    write(
        "ops.json",
        Json::Array(
            d.ops
                .iter()
                .map(|o| {
                    Json::Object(vec![
                        ("uuid".into(), uuid_str(o.uuid)),
                        ("name".into(), Json::String(o.name.clone())),
                        ("level".into(), Json::Number(o.level as f64)),
                    ])
                })
                .collect(),
        ),
    )?;
    let groups = d.groups.iter().map(|(n, g)| {
        (
            n.clone(),
            Json::Object(vec![
                ("permissions".into(), str_array(&g.nodes)),
                ("inherits".into(), str_array(&g.inherits)),
            ]),
        )
    });
    let users = d.users.iter().map(|(id, u)| {
        (
            format_uuid(*id),
            Json::Object(vec![
                ("name".into(), Json::String(u.name.clone())),
                ("groups".into(), str_array(&u.groups)),
                ("permissions".into(), str_array(&u.nodes)),
            ]),
        )
    });
    write(
        "permissions.json",
        Json::Object(vec![
            ("groups".into(), Json::Object(groups.collect())),
            ("users".into(), Json::Object(users.collect())),
        ]),
    )?;
    write(
        "banned-players.json",
        Json::Array(
            d.bans
                .iter()
                .map(|b| {
                    Json::Object(vec![
                        ("uuid".into(), uuid_str(b.uuid)),
                        ("name".into(), Json::String(b.name.clone())),
                        ("reason".into(), Json::String(b.reason.clone())),
                        ("source".into(), Json::String(b.by.clone())),
                    ])
                })
                .collect(),
        ),
    )?;
    write(
        "banned-ips.json",
        Json::Array(
            d.ip_bans
                .iter()
                .map(|b| {
                    Json::Object(vec![
                        ("ip".into(), Json::String(b.ip.clone())),
                        ("reason".into(), Json::String(b.reason.clone())),
                    ])
                })
                .collect(),
        ),
    )?;
    write(
        "whitelist.json",
        Json::Object(vec![
            ("enabled".into(), Json::Bool(d.whitelist_enabled)),
            (
                "players".into(),
                Json::Array(
                    d.whitelist
                        .iter()
                        .map(|(u, n)| {
                            Json::Object(vec![
                                ("uuid".into(), uuid_str(*u)),
                                ("name".into(), Json::String(n.clone())),
                            ])
                        })
                        .collect(),
                ),
            ),
        ]),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn perms() -> Permissions {
        Permissions::open(None, false).unwrap()
    }

    #[test]
    fn op_level_is_the_fallback() {
        let p = perms();
        assert!(!p.has(1, "dangu.command.give", 2));
        p.update(|d| {
            d.ops.push(Op {
                uuid: 1,
                name: "A".into(),
                level: 2,
            })
        });
        assert!(p.has(1, "dangu.command.give", 2));
        assert!(!p.has(1, "dangu.command.stop", 4));
    }

    #[test]
    fn nodes_groups_wildcards_and_negation() {
        let p = perms();
        p.update(|d| {
            d.groups.insert(
                "builder".into(),
                Group {
                    nodes: vec!["dangu.command.*".into(), "-dangu.command.kill".into()],
                    inherits: vec![],
                },
            );
            d.groups.insert(
                "default".into(),
                Group {
                    nodes: vec!["dangu.command.spawn".into()],
                    inherits: vec![],
                },
            );
            d.user_mut(7, "B").groups.push("builder".into());
        });
        assert!(p.has(7, "dangu.command.give", 4), "wildcard from group");
        assert!(!p.has(7, "dangu.command.kill", 0), "negated in group");
        assert!(p.has(99, "dangu.command.spawn", 4), "default group");
        // 사람에게 직접 준 것이 그룹보다 먼저
        p.update(|d| d.user_mut(7, "B").nodes.push("dangu.command.kill".into()));
        assert!(p.has(7, "dangu.command.kill", 4));
        p.update(|d| d.user_mut(7, "B").nodes.push("-*".into()));
        assert!(p.has(7, "dangu.command.kill", 4), "exact beats wildcard");
        assert!(!p.has(7, "dangu.command.give", 0), "user -* beats group");
    }

    #[test]
    fn files_round_trip() {
        let dir = std::env::temp_dir().join(format!("dangu-perms-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let p = Permissions::open(Some(dir.clone()), false).unwrap();
        p.update(|d| {
            d.ops.push(Op {
                uuid: 5,
                name: "Admin".into(),
                level: 3,
            });
            d.groups.insert(
                "vip".into(),
                Group {
                    nodes: vec!["dangu.command.item".into()],
                    inherits: vec!["default".into()],
                },
            );
            d.user_mut(6, "Vip").groups.push("vip".into());
            d.bans.push(Ban {
                uuid: 8,
                name: "Bad".into(),
                reason: "griefing".into(),
                by: "Admin".into(),
            });
            d.ip_bans.push(IpBan {
                ip: "10.0.0.9".into(),
                reason: "".into(),
            });
            d.whitelist_enabled = true;
            d.whitelist.push((6, "Vip".into()));
        });
        let before = p.read(|d| d.clone());
        let again = Permissions::open(Some(dir.clone()), false).unwrap();
        assert_eq!(again.read(|d| d.clone()), before);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
