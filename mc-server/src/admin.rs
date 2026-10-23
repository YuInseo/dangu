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
    /// 권한 노드: `hoya.command.<이름>`.
    pub fn node(&self) -> String {
        format!("hoya.command.{}", self.name)
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
        name: "locate",
        aliases: &[],
        level: 2,
        usage: "/locate structure|biome <이름> — 가장 가까운 구조물·바이옴",
    },
    Spec {
        name: "server",
        aliases: &["join"],
        level: 0,
        usage: "/server [이름] — 인스턴스 목록, 또는 그 인스턴스로 이동",
    },
    Spec {
        name: "instance",
        aliases: &[],
        level: 4,
        usage: "/instance list|create <이름> [시드] [모드] [난이도] [hardcore]|remove <이름>|default <이름>",
    },
    Spec {
        name: "stop",
        aliases: &[],
        level: 4,
        usage: "/stop — 저장하고 끄기",
    },
    Spec {
        name: "재화",
        aliases: &["money", "balance", "돈"],
        level: 0,
        usage: "/재화 [보내기 <플레이어> <양>] — 돈·캐시·잠수포인트, 생활 직업",
    },
    Spec {
        name: "ia",
        aliases: &["itemsadder"],
        level: 2,
        usage: "/ia — 사용자 정의 아이템 목록 (눌러서 받기)",
    },
    Spec {
        name: "iaget",
        aliases: &[],
        level: 2,
        usage: "/iaget <이름공간:아이디> [개수] — 사용자 정의 아이템 받기",
    },
    Spec {
        name: "iareload",
        aliases: &["iazip", "reload", "리로드"],
        level: 4,
        usage: "/iareload — contents 폴더를 다시 읽고 리소스 팩을 새로 만들어 보냄",
    },
    Spec {
        name: "iaimage",
        aliases: &["이모지"],
        level: 0,
        usage: "/iaimage — 채팅에 쓸 수 있는 :이모지: 목록",
    },
    Spec {
        name: "npc",
        aliases: &["엔피시"],
        level: 2,
        usage: "/npc create|select|list|info|rename|type|skin|desc|tphere|look|msg|cmd|action|remove — NPC 만들기 (/npc help)",
    },
    Spec {
        name: "채널",
        aliases: &["ch", "channel"],
        level: 0,
        usage: "/채널 [전체|채널|주변|거래|길드|관리자] | 끄기|켜기 <이름> — 채팅 채널 (앞에 ! $ @ # ~ 를 붙이면 한 번만)",
    },
    Spec {
        name: "장비",
        aliases: &["equip", "gear"],
        level: 0,
        usage: "/장비 — 장비 창 (악세서리·능력치·세트) · /장비 지급 <id> [개수] (OP)",
    },
    Spec {
        name: "강화",
        aliases: &["enhance", "손질", "장비손질", "forge"],
        level: 0,
        usage: "/강화 [강화|연마|조율|전승|분해] — 장비 손질 창 (강화 +30·장인의 기운, 잠재능력 연마·조율, 전승, 분해)",
    },
    Spec {
        name: "대장간",
        aliases: &["smith", "제작"],
        level: 0,
        usage: "/대장간 [제련|장비|악세서리|재료|상점]",
    },
    Spec {
        name: "보스",
        aliases: &["boss"],
        level: 0,
        usage: "/보스 목록 · /보스 소환 <id> (OP)",
    },
    Spec {
        name: "퀘스트",
        aliases: &["quest", "q"],
        level: 0,
        usage: "/퀘스트 [일반|주간|길드] — 퀘스트 창",
    },
    Spec {
        name: "길드",
        aliases: &["guild", "g"],
        level: 0,
        usage: "/길드 [만들기|가입|탈퇴|정보|목록|창고|스폰|땅|설명|방문객|강퇴|위임|부길드장|해산] — 길드 (창은 /길드)",
    },
    Spec {
        name: "hud",
        aliases: &["허드"],
        level: 0,
        usage: "/hud — 오른쪽 재화·스탯 판 끄기/켜기",
    },
    Spec {
        name: "복권",
        aliases: &["lottery", "로또"],
        level: 0,
        usage: "/복권 [정보|추첨] — 복권 판매소 열기, 이번 회차 정보, (OP 4) 지금 추첨",
    },
    Spec {
        name: "등급",
        aliases: &["rank", "후원"],
        level: 0,
        usage: "/등급 — 후원 등급과 특혜 · /등급 목록 · (OP) /등급 설정 <이름> <등급|없음> [일] · /등급 플라이 <이름> <분> · /등급 자동심기 <이름> <개수>",
    },
    Spec {
        name: "메뉴",
        aliases: &["menu", "퀵메뉴"],
        level: 0,
        usage: "/메뉴 — 퀵메뉴 (Shift+F): 서버 이동·내 정보·설정·쓰레기통",
    },
    Spec {
        name: "내정보",
        aliases: &["info", "정보"],
        level: 0,
        usage: "/내정보 — 이름·등급·길드·재화, 생활·모험가 스탯",
    },
    Spec {
        name: "직업",
        aliases: &["jobs"],
        level: 0,
        usage: "/직업 — 생활 스탯 (광부·농부·나무꾼·대장장이)",
    },
    Spec {
        name: "모험가",
        aliases: &["adventurer", "스탯"],
        level: 0,
        usage: "/모험가 — 모험가 스탯 (공격력·마법력·방어력·치명타)",
    },
    Spec {
        name: "쓰레기통",
        aliases: &["trash", "disposal"],
        level: 0,
        usage: "/쓰레기통 — 넣고 닫으면 사라짐",
    },
    Spec {
        name: "업데이트",
        aliases: &["update"],
        level: 4,
        usage: "/업데이트 — 새 버전이 있으면 받아서 저장하고 다시 켬 (릴리스 빌드만)",
    },
    Spec {
        name: "상점",
        aliases: &["shop", "샵"],
        level: 0,
        usage: "/상점 [general|cash|afk] — 돈·캐시·잠수 포인트로 사고팔기",
    },
    Spec {
        name: "ec",
        aliases: &["엔더상자", "enderchest"],
        level: 0,
        usage: "/ec — 개인 엔더상자 (후원 등급)",
    },
    Spec {
        name: "제작대",
        aliases: &["workbench", "wb"],
        level: 0,
        usage: "/제작대 — 어디서나 작업대 (후원 등급)",
    },
    Spec {
        name: "수리",
        aliases: &["repair"],
        level: 0,
        usage: "/수리 — 손에 든 장비 수리 (후원 등급)",
    },
    Spec {
        name: "셋홈",
        aliases: &["sethome"],
        level: 0,
        usage: "/셋홈 [이름] — 지금 자리를 홈으로 (기본 1개, 후원 등급이면 더)",
    },
    Spec {
        name: "홈",
        aliases: &["home"],
        level: 0,
        usage: "/홈 [이름] — 홈으로 이동",
    },
    Spec {
        name: "홈목록",
        aliases: &["homes"],
        level: 0,
        usage: "/홈목록 — 내 홈들",
    },
    Spec {
        name: "홈삭제",
        aliases: &["delhome"],
        level: 0,
        usage: "/홈삭제 <이름>",
    },
    Spec {
        name: "갑옷숨기기",
        aliases: &["hidearmor"],
        level: 0,
        usage: "/갑옷숨기기 — 남에게 내 갑옷을 숨기기/보이기 (후원 등급)",
    },
    Spec {
        name: "플라이",
        aliases: &["fly"],
        level: 0,
        usage: "/플라이 — 남은 플라이 시간 · (OP) /플라이 확인|설정|추가|빼기 <이름> [1h30m20s] · /플라이 목록",
    },
    Spec {
        name: "이동",
        aliases: &["move", "goto"],
        level: 0,
        usage: "/이동 [인스턴스] — 인스턴스 목록, 또는 그쪽으로 이동",
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
    if let Some(p) = server.find_player_anywhere(name) {
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

/// `/플라이 확인|설정|추가|빼기 <이름> [시간]`, `/플라이 목록`. 시간은 `1h30m20s`·`1시간30분`·
/// `1:30:20`·`3600`(초).
fn fly_admin(server: &Server, actor: &Actor, sub: &str, rest: &[&str]) -> Vec<String> {
    use crate::plugins::rank::{self, duration_text, parse_duration};
    if actor.level(server) < 4 {
        return vec!["플라이를 보고 바꾸는 것은 OP 4만 할 수 있습니다.".into()];
    }
    let usage = || {
        vec![
            "사용법: /플라이 확인 <이름> · 설정|추가|빼기 <이름> <시간> · 목록".to_owned(),
            "시간: 1h30m20s · 1시간30분20초 · 1:30:20 · 2d · 3600(초)".to_owned(),
        ]
    };
    if matches!(sub, "목록" | "list") {
        let t = rank::now();
        let mut rows: Vec<(String, i64)> = rank::members().read(|d| {
            d.players
                .iter()
                .filter(|(_, m)| m.fly_until > t)
                .map(|(u, m)| {
                    let name = if m.name.is_empty() {
                        format!("{u:032x}")
                    } else {
                        m.name.clone()
                    };
                    (name, m.fly_until - t)
                })
                .collect()
        });
        rows.sort_by_key(|r| -r.1);
        if rows.is_empty() {
            return vec!["플라이 시간이 남은 사람이 없습니다.".into()];
        }
        return rows
            .into_iter()
            .map(|(n, s)| format!("{n} — {}", duration_text(s)))
            .collect();
    }
    let Some(target) = rest.first() else {
        return usage();
    };
    let Some((uuid, name)) = rank::uuid_by_name(target).or_else(|| resolve(server, target)) else {
        return vec![format!("{target}님을 찾을 수 없습니다.")];
    };
    let amount = || {
        rest.get(1..)
            .map(|r| r.join(""))
            .and_then(|t| parse_duration(&t))
    };
    let (left, what) = match sub {
        "확인" | "info" => {
            return vec![format!(
                "{name}님의 플라이: {} 남음",
                duration_text(rank::fly_left(uuid))
            )];
        }
        "설정" | "set" => match amount() {
            Some(s) => (
                rank::set_fly_secs(uuid, s),
                format!("{}로 설정", duration_text(s)),
            ),
            None => return usage(),
        },
        "추가" | "add" => match amount() {
            Some(s) => (
                rank::add_fly_secs(uuid, s),
                format!("{} 추가", duration_text(s)),
            ),
            None => return usage(),
        },
        "빼기" | "remove" | "take" => match amount() {
            Some(s) => (
                rank::add_fly_secs(uuid, -s),
                format!("{} 뺌", duration_text(s)),
            ),
            None => return usage(),
        },
        _ => return usage(),
    };
    server.publish_all(Event::Whisper {
        to: uuid,
        sender: actor.name.clone(),
        content: format!("플라이 {what} → {} 남음", duration_text(left)),
    });
    vec![format!(
        "{name}님의 플라이 {what}: 이제 {} 남음",
        duration_text(left)
    )]
}

/// `/등급 목록|설정|플라이|자동심기`.
fn rank_admin(server: &Server, actor: &Actor, sub: &str, rest: &[&str]) -> Vec<String> {
    use crate::plugins::rank;
    if sub == "목록" || sub == "list" {
        return rank::get_config()
            .ranks
            .iter()
            .map(|r| {
                format!(
                    "{} ({}) {}원 — {}",
                    r.name,
                    r.id,
                    r.price,
                    r.perks.lines().join(", ")
                )
            })
            .collect();
    }
    if actor.level(server) < 4 {
        return vec!["등급을 주고 바꾸는 것은 OP 4만 할 수 있습니다.".into()];
    }
    let Some((uuid, name)) = rest.first().and_then(|n| resolve(server, n)) else {
        return vec!["사용법: /등급 설정 <이름> <등급|없음> [일] · 플라이 <이름> <분> · 자동심기 <이름> <개수>".into()];
    };
    let number = |i: usize| rest.get(i).and_then(|n| n.parse::<i64>().ok());
    match sub {
        "설정" | "set" => {
            let Some(id) = rest.get(1) else {
                return vec!["사용법: /등급 설정 <이름> <등급|없음> [일]".into()];
            };
            let days = number(2).unwrap_or(0).max(0);
            if matches!(*id, "없음" | "none") {
                rank::grant(uuid, None, 0);
                return vec![format!("{name}님의 등급을 없앴습니다.")];
            }
            let Some(def) = rank::config(|c| c.rank(id).cloned()) else {
                return vec![format!("모르는 등급: {id} (/등급 목록)")];
            };
            rank::grant(uuid, Some(&def), days);
            server.publish_all(Event::System(text::colored(
                &format!("{name}님이 {} 등급이 되었습니다!", def.name),
                "gold",
            )));
            let span = if days > 0 {
                format!("{days}일")
            } else {
                "영구".into()
            };
            vec![format!(
                "{name}님에게 {} 등급을 주었습니다 ({span}).",
                def.name
            )]
        }
        "플라이" | "fly" => match number(1) {
            Some(m) if m > 0 => {
                rank::add_fly_minutes(uuid, m);
                vec![format!("{name}님에게 플라이 {m}분을 주었습니다.")]
            }
            _ => vec!["사용법: /등급 플라이 <이름> <분>".into()],
        },
        "자동심기" | "autoplant" => match number(1) {
            Some(n) if n > 0 => {
                rank::add_autoplant(uuid, n as u64);
                vec![format!("{name}님에게 자동심기 {n}블록을 주었습니다.")]
            }
            _ => vec!["사용법: /등급 자동심기 <이름> <개수>".into()],
        },
        _ => vec!["사용법: /등급 [목록|설정|플라이|자동심기]".into()],
    }
}

/// 관리 명령어를 실행합니다. 관리 명령어가 아니면 None. 결과는 친 쪽에 보일 줄들.
pub fn run(server: &Server, actor: &Actor, name: &str, args: &[&str]) -> Option<Vec<String>> {
    let Some(spec) = spec(name) else {
        return Some(vec![format!("모르는 명령어: /{name} (/help 참고)")]);
    };
    let handled = matches!(
        spec.name,
        "instance"
            | "op"
            | "deop"
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
    ) || (spec.name == "gamemode" && args.len() >= 2)
        || (spec.name == "복권" && !args.is_empty())
        || (spec.name == "등급" && !args.is_empty())
        || (spec.name == "플라이" && !args.is_empty())
        || spec.name == "iareload"
        || spec.name == "업데이트";
    if !handled {
        return None;
    }
    if !actor.allowed(server, spec) {
        return Some(vec!["이 명령어를 쓸 권한이 없습니다.".into()]);
    }
    let usage = || vec![format!("사용법: {}", spec.usage)];
    let rest = |from: usize| args.get(from..).map(|a| a.join(" ")).unwrap_or_default();
    let changed = |uuid: u128| server.publish_all(Event::PermissionsChanged { uuid: Some(uuid) });

    Some(match (spec.name, args) {
        ("iareload", _) => crate::plugins::contents::apply(server),
        ("업데이트", _) => {
            if !crate::updater::enabled() {
                vec![match crate::updater::BUILD {
                    None => {
                        "직접 빌드한 판이라 자동 업데이트를 하지 않습니다 (릴리스의 실행 파일만)."
                            .into()
                    }
                    Some(_) => "자동 업데이트가 꺼져 있습니다 (MC_AUTO_UPDATE).".into(),
                }]
            } else {
                crate::updater::request_check_now();
                vec![format!(
                    "새 버전을 확인합니다 (지금 {}). 있으면 저장하고 다시 켭니다.",
                    crate::updater::short(crate::updater::BUILD.unwrap_or("?"))
                )]
            }
        }
        ("복권", [sub, ..]) => {
            use crate::plugins::lottery::{self, format_short, numbers_text, won};
            let l = &server.lottery;
            match *sub {
                "정보" | "info" => {
                    let mut v = vec![
                        format!(
                            "[{}] 제{}회 복권 — 추첨 {}",
                            server.name,
                            l.round(),
                            format_short(l.next_draw())
                        ),
                        format!(
                            "판매 {}장 · 1등 예상 당첨금 {}",
                            l.sales(),
                            won(l.jackpot())
                        ),
                    ];
                    if let Some(d) = l.last_draw() {
                        v.push(format!(
                            "지난 제{}회: {} (1등 {}명 · 2등 {}명 · 3등 {}명 · 4등 {}명)",
                            d.round,
                            numbers_text(&d.numbers),
                            d.winners[0],
                            d.winners[1],
                            d.winners[2],
                            d.winners[3]
                        ));
                    }
                    v
                }
                "추첨" | "draw" => {
                    if actor.level(server) < 4 {
                        return Some(vec!["지금 추첨은 OP 4만 할 수 있습니다.".into()]);
                    }
                    let d = l.draw(None, lottery::now());
                    lottery::announce(server, &d);
                    vec![format!(
                        "제{}회 추첨: {}",
                        d.round,
                        numbers_text(&d.numbers)
                    )]
                }
                _ => usage(),
            }
        }
        ("등급", [sub, rest @ ..]) => rank_admin(server, actor, sub, rest),
        ("플라이", [sub, rest @ ..]) => fly_admin(server, actor, sub, rest),
        ("list", _) => {
            let mut lines = Vec::new();
            for inst in server.instances() {
                let names: Vec<_> = inst.players().into_iter().map(|p| p.name).collect();
                lines.push(format!(
                    "[{}] {}명: {}",
                    inst.name,
                    names.len(),
                    names.join(", ")
                ));
            }
            if lines.len() > 1 {
                lines.insert(0, format!("모두 {}명", server.online_total()));
                return Some(lines);
            }
            let names: Vec<_> = server.players().into_iter().map(|p| p.name).collect();
            vec![format!("접속자 {}명: {}", names.len(), names.join(", "))]
        }
        ("say", [_, ..]) => {
            crate::plugins::chat::log::record(
                "공지",
                &server.name,
                &actor.name,
                None,
                None,
                &rest(0),
            );
            server.publish_all(Event::System(text::colored(
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
                server.publish_all(Event::Title {
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
            match (
                GameMode::from_name(mode),
                server.find_player_anywhere(target),
            ) {
                (Some(mode), Some(p)) => {
                    server.publish_all(Event::SetGameMode {
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
        ("kick", [target, ..]) => match server.find_player_anywhere(target) {
            Some(p) => {
                let reason = if args.len() > 1 {
                    rest(1)
                } else {
                    "관리자가 내보냈습니다.".into()
                };
                server.publish_all(Event::Kick {
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
                server.publish_all(Event::Kick {
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
            for p in server.players_anywhere().into_iter().filter(|p| p.ip == ip) {
                server.publish_all(Event::Kick {
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
        ("instance", sub) => instance(server, sub),
        ("save-all", _) => {
            match server.network() {
                Some(net) => net.save_all(),
                None => server.save_all(),
            }
            vec!["저장했습니다.".into()]
        }
        ("stop", _) => {
            server.publish_all(Event::System(text::colored("서버를 끕니다…", "red")));
            server.shutdown.notify_one();
            vec!["끄는 중…".into()]
        }
        _ => usage(),
    })
}

fn instance(server: &Server, args: &[&str]) -> Vec<String> {
    use crate::network::InstanceSpec;
    let Some(net) = server.network() else {
        return vec!["네트워크 없이 혼자 도는 서버입니다.".into()];
    };
    match args {
        [] | ["list"] => {
            let default = net.default_name();
            net.specs()
                .into_iter()
                .map(|s| {
                    let online = net.get(&s.name).map_or(0, |i| i.online());
                    format!(
                        "{}{} — 시드 {}, {}, {}{}, {}명",
                        s.name,
                        if s.name == default { " (기본)" } else { "" },
                        s.seed,
                        s.gamemode.display_name(),
                        s.difficulty.display_name(),
                        if s.hardcore { ", 하드코어" } else { "" },
                        online
                    )
                })
                .collect()
        }
        ["create", name, rest @ ..] => {
            let seed = (crate::game::rand_u32() as u64) << 32 | crate::game::rand_u32() as u64;
            let mut spec = InstanceSpec::new(name, seed);
            for a in rest {
                if let Ok(seed) = a.parse::<u64>() {
                    spec.seed = seed;
                } else if let Some(m) = GameMode::from_name(a) {
                    spec.gamemode = m;
                } else if let Some(d) = Difficulty::from_name(a) {
                    spec.difficulty = d;
                } else if *a == "hardcore" {
                    spec.hardcore = true;
                } else {
                    return vec![format!(
                        "모르는 옵션: {a} (시드 숫자, 게임 모드, 난이도, hardcore)"
                    )];
                }
            }
            match net.create(spec.clone()) {
                Ok(_) => vec![format!(
                    "인스턴스 {} 만듦 (시드 {}). /server {}로 갈 수 있습니다.",
                    spec.name, spec.seed, spec.name
                )],
                Err(e) => vec![e],
            }
        }
        ["remove", name] => match net.remove(name) {
            Ok(()) => vec![format!(
                "인스턴스 {name}을(를) 내렸습니다 (폴더는 남아 있음)."
            )],
            Err(e) => vec![e],
        },
        ["default", name] => match net.set_default(name) {
            Ok(()) => vec![format!("이제 처음 접속하면 {name}(으)로 들어갑니다.")],
            Err(e) => vec![e],
        },
        _ => vec![spec("instance").unwrap().usage.into()],
    }
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
    let notify = |uuid: Option<u128>| server.publish_all(Event::PermissionsChanged { uuid });
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
                let level = spec(node.trim_start_matches("hoya.command.")).map_or(4, |s| s.level);
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
    let instance_names = || {
        server
            .network()
            .map(|n| n.specs().into_iter().map(|s| s.name).collect())
            .unwrap_or_default()
    };
    let mut out: Vec<String> = match (spec.name, &words[1..]) {
        ("server", []) => instance_names(),
        ("iaget", []) => crate::plugins::contents::current().items.clone(),
        ("장비", ["지급"]) => crate::plugins::equip::config::with(|c| {
            c.gear
                .iter()
                .map(|g| g.id.clone())
                .chain(c.materials.iter().map(|m| m.id.clone()))
                .chain(c.ores.iter().map(|o| o.block.clone()))
                .collect()
        }),
        ("보스", ["소환"]) => {
            crate::plugins::equip::config::with(|c| c.bosses.iter().map(|b| b.id.clone()).collect())
        }
        ("npc", []) => [
            "create", "select", "list", "info", "rename", "type", "skin", "desc", "tphere", "look",
            "msg", "cmd", "action", "remove",
        ]
        .map(String::from)
        .to_vec(),
        ("locate", []) => ["structure", "biome"].map(String::from).to_vec(),
        ("locate", ["structure"]) => crate::world::structure::names()
            .into_iter()
            .map(String::from)
            .collect(),
        ("locate", ["biome"]) => crate::world::biome::all()
            .map(|b| b.name().to_owned())
            .collect(),
        ("instance", []) => ["list", "create", "remove", "default"]
            .map(String::from)
            .to_vec(),
        ("instance", ["remove" | "default"]) => instance_names(),
        ("instance", ["create", _, ..]) => GameMode::ALL
            .iter()
            .map(|m| m.name().to_owned())
            .chain(Difficulty::ALL.iter().map(|d| d.name().to_owned()))
            .chain(["hardcore".to_owned()])
            .collect(),
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
                .chain(["hoya.command.*".to_owned(), "*".to_owned()])
                .collect()
        }
        _ => Vec::new(),
    };
    out.retain(|s| s.to_lowercase().starts_with(&prefix.to_lowercase()));
    out.sort();
    out.dedup();
    (start, out)
}
