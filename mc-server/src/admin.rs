//! 명령어 표(권한 노드와 기본 OP 단계)와, 플레이어·콘솔이 함께 쓰는 관리 명령어.
//!
//! 플레이어 자신에게 하는 명령어(/give, /tp, …)는 [`crate::session`]의 세션이 실행하고,
//! 여기 있는 것은 누가 치든 같은 일을 하는 것들입니다: /op /deop /kick /ban /pardon
//! /ban-ip /pardon-ip /whitelist /perm /say /list /difficulty /stop /save-all, 그리고
//! 다른 사람을 대상으로 하는 /gamemode.

use crate::packets::play::GameMode;
use crate::perms::{Ban, IpBan, Op};
use crate::server::{Difficulty, Event, Server};
use crate::text;

/// 명령어 하나의 권한.
#[derive(Debug, Clone, Copy)]
pub struct Spec {
    pub name: &'static str,
    pub aliases: &'static [&'static str],
    /// 기본 OP 단계 (권한 노드 규칙이 없을 때).
    pub level: u8,
    pub usage: &'static str,
}

impl Spec {
    /// 권한 노드: `dangu.command.<이름>`.
    pub fn node(&self) -> String {
        format!("dangu.command.{}", self.name)
    }
}

pub const COMMANDS: &[Spec] = &[
    Spec {
        name: "help",
        aliases: &[],
        level: 0,
        usage: "/help — 쓸 수 있는 명령어",
    },
    Spec {
        name: "list",
        aliases: &[],
        level: 0,
        usage: "/list — 접속자",
    },
    Spec {
        name: "msg",
        aliases: &["tell", "w"],
        level: 0,
        usage: "/msg <이름> <내용> — 귓속말",
    },
    Spec {
        name: "me",
        aliases: &[],
        level: 0,
        usage: "/me <행동>",
    },
    Spec {
        name: "spawn",
        aliases: &[],
        level: 0,
        usage: "/spawn — 이 차원의 스폰으로",
    },
    Spec {
        name: "overworld",
        aliases: &["nether", "end"],
        level: 0,
        usage: "/overworld /nether /end — 차원 이동",
    },
    Spec {
        name: "tp",
        aliases: &[],
        level: 2,
        usage: "/tp <이름> — 그 사람에게",
    },
    Spec {
        name: "gamemode",
        aliases: &[],
        level: 2,
        usage: "/gamemode <모드> [이름]",
    },
    Spec {
        name: "time",
        aliases: &[],
        level: 2,
        usage: "/time set <day|noon|night|midnight>",
    },
    Spec {
        name: "give",
        aliases: &[],
        level: 2,
        usage: "/give <아이템> [개수]",
    },
    Spec {
        name: "item",
        aliases: &[],
        level: 2,
        usage: "/item <사용자 정의 아이템> | list | reload",
    },
    Spec {
        name: "summon",
        aliases: &[],
        level: 2,
        usage: "/summon <몹>",
    },
    Spec {
        name: "kill",
        aliases: &[],
        level: 2,
        usage: "/kill",
    },
    Spec {
        name: "difficulty",
        aliases: &[],
        level: 2,
        usage: "/difficulty <peaceful|easy|normal|hard>",
    },
    Spec {
        name: "say",
        aliases: &[],
        level: 2,
        usage: "/say <내용> — 모두에게 알림",
    },
    Spec {
        name: "title",
        aliases: &[],
        level: 2,
        usage: "/title <이름|@a> <내용> — 화면 가운데 큰 글씨",
    },
    Spec {
        name: "kick",
        aliases: &[],
        level: 3,
        usage: "/kick <이름> [사유]",
    },
    Spec {
        name: "ban",
        aliases: &[],
        level: 3,
        usage: "/ban <이름> [사유]",
    },
    Spec {
        name: "pardon",
        aliases: &["unban"],
        level: 3,
        usage: "/pardon <이름>",
    },
    Spec {
        name: "ban-ip",
        aliases: &[],
        level: 3,
        usage: "/ban-ip <IP|이름> [사유]",
    },
    Spec {
        name: "pardon-ip",
        aliases: &[],
        level: 3,
        usage: "/pardon-ip <IP>",
    },
    Spec {
        name: "whitelist",
        aliases: &[],
        level: 3,
        usage: "/whitelist on|off|list|add <이름>|remove <이름>",
    },
    Spec {
        name: "op",
        aliases: &[],
        level: 3,
        usage: "/op <이름> [단계 1~4]",
    },
    Spec {
        name: "deop",
        aliases: &[],
        level: 3,
        usage: "/deop <이름>",
    },
    Spec {
        name: "perm",
        aliases: &[],
        level: 3,
        usage: "/perm user|group|check|info … — 권한 노드",
    },
    Spec {
        name: "save-all",
        aliases: &[],
        level: 4,
        usage: "/save-all — 지금 저장",
    },
    Spec {
        name: "stop",
        aliases: &[],
        level: 4,
        usage: "/stop — 저장하고 끄기",
    },
];

pub fn spec(name: &str) -> Option<&'static Spec> {
    COMMANDS
        .iter()
        .find(|s| s.name == name || s.aliases.contains(&name))
}

/// 명령어를 친 쪽.
pub struct Actor {
    pub name: String,
    /// 콘솔이면 None (모든 권한).
    pub uuid: Option<u128>,
}

impl Actor {
    pub fn console() -> Self {
        Self {
            name: "콘솔".into(),
            uuid: None,
        }
    }

    pub fn level(&self, server: &Server) -> u8 {
        self.uuid.map_or(4, |u| server.perms.op_level(u))
    }

    pub fn allowed(&self, server: &Server, spec: &Spec) -> bool {
        self.uuid
            .is_none_or(|u| server.perms.has(u, &spec.node(), spec.level))
    }
}

/// 이름 → (UUID, 정확한 이름). 접속 중인 사람, 기록에 있는 사람, (오프라인 모드면) 이름에서
/// 만든 UUID 순으로 찾습니다.
pub fn resolve(server: &Server, name: &str) -> Option<(u128, String)> {
    if let Some(p) = server.find_player(name) {
        return Some((p.uuid, p.name));
    }
    if let Some(found) = server.perms.read(|d| d.find_name(name)) {
        return Some(found);
    }
    let valid = !name.is_empty()
        && name.len() <= 16
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
    (!server.config.online_mode && valid)
        .then(|| (crate::uuid::offline_uuid(name), name.to_owned()))
}

/// 관리 명령어를 실행합니다. 관리 명령어가 아니면 None. 결과는 친 쪽에 보일 줄들.
pub fn run(server: &Server, actor: &Actor, name: &str, args: &[&str]) -> Option<Vec<String>> {
    let Some(spec) = spec(name) else {
        return Some(vec![format!("모르는 명령어: /{name} (/help 참고)")]);
    };
    let handled = matches!(
        spec.name,
        "op" | "deop"
            | "kick"
            | "ban"
            | "pardon"
            | "ban-ip"
            | "pardon-ip"
            | "whitelist"
            | "perm"
            | "say"
            | "list"
            | "difficulty"
            | "stop"
            | "save-all"
            | "title"
    ) || (spec.name == "gamemode" && args.len() >= 2);
    if !handled {
        return None;
    }
    if !actor.allowed(server, spec) {
        return Some(vec!["이 명령어를 쓸 권한이 없습니다.".into()]);
    }
    let usage = || vec![format!("사용법: {}", spec.usage)];
    let rest = |from: usize| args.get(from..).map(|a| a.join(" ")).unwrap_or_default();
    let changed = |uuid: u128| server.publish(Event::PermissionsChanged { uuid: Some(uuid) });

    Some(match (spec.name, args) {
        ("list", _) => {
            let names: Vec<_> = server.players().into_iter().map(|p| p.name).collect();
            vec![format!("접속자 {}명: {}", names.len(), names.join(", "))]
        }
        ("say", [_, ..]) => {
            server.publish(Event::System(text::colored(
                &format!("[{}] {}", actor.name, rest(0)),
                "light_purple",
            )));
            vec![]
        }
        ("title", [target, _, ..]) => {
            let message = rest(1);
            let targets: Vec<u128> = if *target == "@a" {
                server.players().into_iter().map(|p| p.uuid).collect()
            } else {
                server
                    .find_player(target)
                    .into_iter()
                    .map(|p| p.uuid)
                    .collect()
            };
            if targets.is_empty() {
                return Some(vec![format!("{target}님은 접속해 있지 않습니다.")]);
            }
            for uuid in targets {
                server.publish(Event::Title {
                    uuid,
                    title: message.clone(),
                });
            }
            vec![]
        }
        ("difficulty", [d]) => match Difficulty::from_name(d) {
            Some(d) if server.set_difficulty(d) => {
                vec![format!("난이도를 {}(으)로 바꿨습니다.", d.display_name())]
            }
            Some(_) => vec!["난이도가 잠겨 있어 바꿀 수 없습니다.".into()],
            None => usage(),
        },
        ("gamemode", [mode, target, ..]) => {
            match (GameMode::from_name(mode), server.find_player(target)) {
                (Some(mode), Some(p)) => {
                    server.publish(Event::SetGameMode {
                        uuid: p.uuid,
                        gamemode: mode,
                    });
                    vec![format!(
                        "{}님의 게임 모드를 {}(으)로 바꿨습니다.",
                        p.name,
                        mode.display_name()
                    )]
                }
                (None, _) => usage(),
                (_, None) => vec![format!("{target}님은 접속해 있지 않습니다.")],
            }
        }
        ("op", [target, level @ ..]) => {
            let level = level
                .first()
                .and_then(|l| l.parse::<u8>().ok())
                .unwrap_or(4)
                .clamp(1, 4);
            if level > actor.level(server) {
                return Some(vec![format!(
                    "자기 단계({})보다 높은 OP는 줄 수 없습니다.",
                    actor.level(server)
                )]);
            }
            let Some((uuid, name)) = resolve(server, target) else {
                return Some(vec![format!(
                    "{target}님을 찾을 수 없습니다 (정품 모드에서는 한 번 접속해야 합니다)."
                )]);
            };
            server.perms.update(|d| {
                d.ops.retain(|o| o.uuid != uuid);
                d.ops.push(Op {
                    uuid,
                    name: name.clone(),
                    level,
                });
            });
            changed(uuid);
            vec![format!("{name}님을 OP {level}단계로 만들었습니다.")]
        }
        ("deop", [target]) => match resolve(server, target) {
            Some((uuid, name)) => {
                let removed = server.perms.update(|d| {
                    let before = d.ops.len();
                    d.ops.retain(|o| o.uuid != uuid);
                    before != d.ops.len()
                });
                changed(uuid);
                vec![if removed {
                    format!("{name}님은 이제 OP가 아닙니다.")
                } else {
                    format!("{name}님은 OP가 아니었습니다.")
                }]
            }
            None => vec![format!("{target}님을 찾을 수 없습니다.")],
        },
        ("kick", [target, ..]) => match server.find_player(target) {
            Some(p) => {
                let reason = if args.len() > 1 {
                    rest(1)
                } else {
                    "관리자가 내보냈습니다.".into()
                };
                server.publish(Event::Kick {
                    uuid: p.uuid,
                    reason,
                });
                vec![format!("{}님을 내보냈습니다.", p.name)]
            }
            None => vec![format!("{target}님은 접속해 있지 않습니다.")],
        },
        ("ban", [target, ..]) => match resolve(server, target) {
            Some((uuid, name)) => {
                let reason = if args.len() > 1 {
                    rest(1)
                } else {
                    "이 서버에서 차단되었습니다.".into()
                };
                server.perms.update(|d| {
                    d.bans.retain(|b| b.uuid != uuid);
                    d.bans.push(Ban {
                        uuid,
                        name: name.clone(),
                        reason: reason.clone(),
                        by: actor.name.clone(),
                    });
                });
                server.publish(Event::Kick {
                    uuid,
                    reason: format!("차단되었습니다: {reason}"),
                });
                vec![format!("{name}님을 차단했습니다.")]
            }
            None => vec![format!("{target}님을 찾을 수 없습니다.")],
        },
        ("pardon", [target]) => {
            let found = server.perms.update(|d| {
                let before = d.bans.len();
                d.bans.retain(|b| !b.name.eq_ignore_ascii_case(target));
                before != d.bans.len()
            });
            vec![if found {
                format!("{target}님의 차단을 풀었습니다.")
            } else {
                format!("{target}님은 차단되어 있지 않습니다.")
            }]
        }
        ("ban-ip", [target, ..]) => {
            let ip = server
                .find_player(target)
                .map_or_else(|| target.to_string(), |p| p.ip.clone());
            if ip.parse::<std::net::IpAddr>().is_err() {
                return Some(vec![format!("IP 주소가 아닙니다: {ip}")]);
            }
            let reason = if args.len() > 1 {
                rest(1)
            } else {
                "이 IP는 차단되었습니다.".into()
            };
            server.perms.update(|d| {
                d.ip_bans.retain(|b| b.ip != ip);
                d.ip_bans.push(IpBan {
                    ip: ip.clone(),
                    reason: reason.clone(),
                });
            });
            for p in server.players().into_iter().filter(|p| p.ip == ip) {
                server.publish(Event::Kick {
                    uuid: p.uuid,
                    reason: format!("차단되었습니다: {reason}"),
                });
            }
            vec![format!("{ip}을(를) 차단했습니다.")]
        }
        ("pardon-ip", [ip]) => {
            server.perms.update(|d| d.ip_bans.retain(|b| b.ip != *ip));
            vec![format!("{ip}의 차단을 풀었습니다.")]
        }
        ("whitelist", [cmd, rest @ ..]) => match (*cmd, rest) {
            ("on" | "off", _) => {
                let on = *cmd == "on";
                server.perms.update(|d| d.whitelist_enabled = on);
                vec![format!(
                    "화이트리스트를 {}.",
                    if on { "켰습니다" } else { "껐습니다" }
                )]
            }
            ("list", _) => {
                let (on, names) = server.perms.read(|d| {
                    (
                        d.whitelist_enabled,
                        d.whitelist
                            .iter()
                            .map(|(_, n)| n.clone())
                            .collect::<Vec<_>>(),
                    )
                });
                vec![format!(
                    "화이트리스트({}) {}명: {}",
                    if on { "켜짐" } else { "꺼짐" },
                    names.len(),
                    names.join(", ")
                )]
            }
            ("add", [target]) => match resolve(server, target) {
                Some((uuid, name)) => {
                    server.perms.update(|d| {
                        if !d.whitelisted(uuid) {
                            d.whitelist.push((uuid, name.clone()));
                        }
                    });
                    vec![format!("{name}님을 화이트리스트에 넣었습니다.")]
                }
                None => vec![format!("{target}님을 찾을 수 없습니다.")],
            },
            ("remove", [target]) => {
                server
                    .perms
                    .update(|d| d.whitelist.retain(|(_, n)| !n.eq_ignore_ascii_case(target)));
                vec![format!("{target}님을 화이트리스트에서 뺐습니다.")]
            }
            _ => usage(),
        },
        ("perm", sub) => perm(server, sub),
        ("save-all", _) => {
            server.save_all();
            vec!["저장했습니다.".into()]
        }
        ("stop", _) => {
            server.publish(Event::System(text::colored("서버를 끕니다…", "red")));
            server.shutdown.notify_one();
            vec!["끄는 중…".into()]
        }
        _ => usage(),
    })
}

fn perm(server: &Server, args: &[&str]) -> Vec<String> {
    const USAGE: &[&str] = &[
        "/perm user <이름> add|remove <노드>",
        "/perm user <이름> group add|remove <그룹>",
        "/perm group <그룹> add|remove <노드>",
        "/perm group <그룹> inherit <부모 그룹>",
        "/perm check <이름> <노드>",
        "/perm info <이름>",
        "/perm groups",
    ];
    let notify = |uuid: Option<u128>| server.publish(Event::PermissionsChanged { uuid });
    match args {
        ["user", name, "add" | "remove", node] => {
            let Some((uuid, name)) = resolve(server, name) else {
                return vec![format!("{name}님을 찾을 수 없습니다.")];
            };
            let add = args[2] == "add";
            server.perms.update(|d| {
                let u = d.user_mut(uuid, &name);
                u.nodes.retain(|n| n != node);
                if add {
                    u.nodes.push(node.to_string());
                }
            });
            notify(Some(uuid));
            vec![format!("{name}: {} {node}", if add { "+" } else { "−" })]
        }
        ["user", name, "group", "add" | "remove", group] => {
            let Some((uuid, name)) = resolve(server, name) else {
                return vec![format!("{name}님을 찾을 수 없습니다.")];
            };
            let add = args[3] == "add";
            server.perms.update(|d| {
                d.groups.entry(group.to_string()).or_default();
                let u = d.user_mut(uuid, &name);
                u.groups.retain(|g| g != group);
                if add {
                    u.groups.push(group.to_string());
                }
            });
            notify(Some(uuid));
            vec![format!(
                "{name}: 그룹 {} {group}",
                if add { "+" } else { "−" }
            )]
        }
        ["group", group, "add" | "remove", node] => {
            let add = args[2] == "add";
            server.perms.update(|d| {
                let g = d.groups.entry(group.to_string()).or_default();
                g.nodes.retain(|n| n != node);
                if add {
                    g.nodes.push(node.to_string());
                }
            });
            notify(None);
            vec![format!(
                "그룹 {group}: {} {node}",
                if add { "+" } else { "−" }
            )]
        }
        ["group", group, "inherit", parent] => {
            server.perms.update(|d| {
                let g = d.groups.entry(group.to_string()).or_default();
                if !g.inherits.iter().any(|p| p == parent) {
                    g.inherits.push(parent.to_string());
                }
            });
            notify(None);
            vec![format!("그룹 {group}이(가) {parent}을(를) 물려받습니다.")]
        }
        ["check", name, node] => match resolve(server, name) {
            Some((uuid, name)) => {
                let level = spec(node.trim_start_matches("dangu.command.")).map_or(4, |s| s.level);
                let has = server.perms.has(uuid, node, level);
                vec![format!(
                    "{name}: {node} = {}",
                    if has { "있음" } else { "없음" }
                )]
            }
            None => vec![format!("{name}님을 찾을 수 없습니다.")],
        },
        ["info", name] => match resolve(server, name) {
            Some((uuid, name)) => {
                let level = server.perms.op_level(uuid);
                let (groups, nodes) = server.perms.read(|d| {
                    d.users
                        .get(&uuid)
                        .map(|u| (u.groups.join(", "), u.nodes.join(", ")))
                        .unwrap_or_default()
                });
                vec![format!(
                    "{name}: OP {level}단계, 그룹 [{groups}], 노드 [{nodes}]"
                )]
            }
            None => vec![format!("{name}님을 찾을 수 없습니다.")],
        },
        ["groups"] => server.perms.read(|d| {
            d.groups
                .iter()
                .map(|(n, g)| format!("{n}: [{}] ← {}", g.nodes.join(", "), g.inherits.join(", ")))
                .collect()
        }),
        _ => USAGE.iter().map(|s| s.to_string()).collect(),
    }
}

/// 서버가 주는 자동 완성 (`ask_server` 인자). `line`은 `/`를 뺀 입력 중인 줄.
/// 돌려주는 값: (바꿀 마지막 단어의 시작 글자 위치, 후보들).
pub fn suggest(server: &Server, actor: &Actor, line: &str) -> (usize, Vec<String>) {
    let start = line.rfind(' ').map_or(0, |i| i + 1);
    let prefix = &line[start..];
    let words: Vec<&str> = line[..start].split_whitespace().collect();
    let Some(spec) = words.first().and_then(|w| spec(w)) else {
        return (start, Vec::new());
    };
    if !actor.allowed(server, spec) {
        return (start, Vec::new());
    }
    let mut out: Vec<String> = match (spec.name, &words[1..]) {
        ("pardon", []) => server
            .perms
            .read(|d| d.bans.iter().map(|b| b.name.clone()).collect()),
        ("pardon-ip", []) => server
            .perms
            .read(|d| d.ip_bans.iter().map(|b| b.ip.clone()).collect()),
        ("perm", []) => ["user", "group", "check", "info", "groups"]
            .map(String::from)
            .to_vec(),
        ("perm", ["user" | "check" | "info"]) => {
            server.players().into_iter().map(|p| p.name).collect()
        }
        ("perm", ["group"]) => server.perms.read(|d| d.groups.keys().cloned().collect()),
        ("perm", ["user", _]) => ["add", "remove", "group"].map(String::from).to_vec(),
        ("perm", ["group", _]) => ["add", "remove", "inherit"].map(String::from).to_vec(),
        ("perm", ["user", _, "group"]) => ["add", "remove"].map(String::from).to_vec(),
        ("perm", ["user", _, "group", _] | ["group", _, "inherit"]) => {
            server.perms.read(|d| d.groups.keys().cloned().collect())
        }
        ("perm", ["user", _, "add" | "remove"] | ["group", _, "add" | "remove"] | ["check", _]) => {
            COMMANDS
                .iter()
                .map(Spec::node)
                .chain(["dangu.command.*".to_owned(), "*".to_owned()])
                .collect()
        }
        _ => Vec::new(),
    };
    out.retain(|s| s.to_lowercase().starts_with(&prefix.to_lowercase()));
    out.sort();
    out.dedup();
    (start, out)
}
