//! 명령어: 사람마다 다른 문법 트리(권한 있는 것만, 클라이언트 자동 완성용)와 실행.
//! 관리 명령어는 [`crate::admin`]이 실행하고, 여기서는 자기 자신에게 하는 것들을 합니다.

use tokio::io::AsyncWrite;

use super::play::{Session, sanitize};
use crate::admin::{self, Actor};
use crate::data::{argument_type, items};
use crate::game::custom_items;
use crate::game::mobs::{self, MobKind};
use crate::packets::commands::{CommandTree, ROOT};
use crate::packets::play::{ChatKind, GameMode};
use crate::packets::world::Item;
use crate::protocol::frame::FrameError;
use crate::server::{Difficulty, Event, Server};
use crate::text;
use crate::world::{Dimension, terrain};

/// `/time set <이름>`이 받는 시각.
const TIMES: &[(&str, i64)] = &[
    ("day", 1000),
    ("noon", 6000),
    ("night", 13000),
    ("midnight", 18000),
];
/// `brigadier:string` 속성: 0 한 단어, 2 남은 전부.
const WORD: &[u8] = &[0];
const GREEDY: &[u8] = &[2];

/// 이 사람에게 보낼 명령어 트리. 권한이 없는 명령어는 넣지 않습니다 (빨간 글씨로 보이도록).
pub fn tree(server: &Server, uuid: u128) -> CommandTree {
    let actor = Actor {
        name: String::new(),
        uuid: Some(uuid),
    };
    let mut t = CommandTree::default();
    for spec in admin::COMMANDS {
        if !actor.allowed(server, spec) {
            continue;
        }
        for name in std::iter::once(spec.name).chain(spec.aliases.iter().copied()) {
            build(&mut t, spec.name, name);
        }
    }
    t
}

fn build(t: &mut CommandTree, kind: &str, name: &'static str) {
    use argument_type::{GAME_PROFILE, INTEGER, ITEM_STACK, MESSAGE, STRING};
    match kind {
        "help" | "list" | "spawn" | "kill" | "save-all" | "stop" | "overworld" => {
            t.literal(ROOT, name, true);
        }
        "msg" => {
            let c = t.literal(ROOT, name, false);
            let target = t.argument(c, "targets", GAME_PROFILE, false);
            t.argument(target, "message", MESSAGE, true);
        }
        "me" | "say" => {
            let c = t.literal(ROOT, name, false);
            t.argument(c, "message", MESSAGE, true);
        }
        "tp" | "deop" => {
            let c = t.literal(ROOT, name, false);
            t.argument(c, "target", GAME_PROFILE, true);
        }
        "op" => {
            let c = t.literal(ROOT, name, false);
            let target = t.argument(c, "target", GAME_PROFILE, true);
            t.argument_with(target, "level", INTEGER, &[0], true);
        }
        "kick" | "ban" => {
            let c = t.literal(ROOT, name, false);
            let target = t.argument(c, "target", GAME_PROFILE, true);
            t.argument(target, "reason", MESSAGE, true);
        }
        "ban-ip" => {
            let c = t.literal(ROOT, name, false);
            let target = t.argument_with(c, "target", STRING, WORD, true);
            t.argument(target, "reason", MESSAGE, true);
        }
        "pardon" | "pardon-ip" => {
            let c = t.literal(ROOT, name, false);
            t.argument_ask_server(c, "target", STRING, WORD, true);
        }
        "title" => {
            let c = t.literal(ROOT, name, false);
            let target = t.argument_with(c, "target", STRING, WORD, false);
            t.argument(target, "message", MESSAGE, true);
        }
        "gamemode" => {
            let c = t.literal(ROOT, name, false);
            for mode in GameMode::ALL {
                let m = t.literal(c, mode.name(), true);
                t.argument(m, "target", GAME_PROFILE, true);
            }
        }
        "difficulty" => {
            let c = t.literal(ROOT, name, false);
            for d in Difficulty::ALL {
                t.literal(c, d.name(), true);
            }
        }
        "time" => {
            let c = t.literal(ROOT, name, false);
            let set = t.literal(c, "set", false);
            for (n, _) in TIMES {
                t.literal(set, *n, true);
            }
        }
        "give" => {
            let c = t.literal(ROOT, name, false);
            let item = t.argument(c, "item", ITEM_STACK, true);
            t.argument_with(item, "count", INTEGER, &[0], true);
        }
        "item" => {
            let c = t.literal(ROOT, name, false);
            t.literal(c, "list", true);
            t.literal(c, "reload", true);
            for def in custom_items::all() {
                t.literal(c, def.id, true);
            }
        }
        "summon" => {
            let c = t.literal(ROOT, name, false);
            for m in ["cow", "pig", "sheep", "chicken", "zombie", "spider"] {
                t.literal(c, m, true);
            }
        }
        "whitelist" => {
            let c = t.literal(ROOT, name, false);
            for sub in ["on", "off", "list"] {
                t.literal(c, sub, true);
            }
            for sub in ["add", "remove"] {
                let s = t.literal(c, sub, false);
                t.argument_with(s, "target", STRING, WORD, true);
            }
        }
        "server" => {
            let c = t.literal(ROOT, name, true);
            t.argument_ask_server(c, "instance", STRING, WORD, true);
        }
        "instance" => {
            let c = t.literal(ROOT, name, true);
            t.argument_ask_server(c, "args", STRING, GREEDY, true);
        }
        "perm" => {
            let c = t.literal(ROOT, name, true);
            t.argument_ask_server(c, "args", STRING, GREEDY, true);
        }
        _ => {}
    }
}

impl<W: AsyncWrite + Unpin> Session<'_, W> {
    pub(super) async fn command(&mut self, line: &str) -> Result<(), FrameError> {
        let (name, rest) = line.split_once(' ').unwrap_or((line, ""));
        let rest = rest.trim();
        let args: Vec<&str> = rest.split_whitespace().collect();

        let Some(spec) = admin::spec(name) else {
            return self
                .error(&format!("모르는 명령어: /{name} (/help 참고)"))
                .await;
        };
        let actor = Actor {
            name: self.profile.name.clone(),
            uuid: Some(self.profile.uuid),
        };
        if !actor.allowed(self.server, spec) {
            return self.error("이 명령어를 쓸 권한이 없습니다.").await;
        }
        // 누가 치든 같은 관리 명령어
        if let Some(lines) = admin::run(self.server, &actor, name, &args) {
            for l in lines {
                self.message(&text::plain(&l)).await?;
            }
            return Ok(());
        }

        if let Some(dimension) = Dimension::from_command(name) {
            return self.travel(dimension).await;
        }
        match spec.name {
            "server" => self.server_command(args.first().copied()).await,
            "help" => {
                for s in admin::COMMANDS
                    .iter()
                    .filter(|s| actor.allowed(self.server, s))
                {
                    self.message(&text::plain(s.usage)).await?;
                }
                Ok(())
            }
            "give" => {
                let (spec, count) = rest.split_once(' ').unwrap_or((rest, "1"));
                // `minecraft:diamond_sword[...]`에서 이름만
                let id_text = spec.split(['[', '{']).next().unwrap_or("");
                let Some(id) = items::by_name(id_text).filter(|&id| id > 0) else {
                    return self
                        .error(&format!(
                            "모르는 아이템: {id_text} (우리만의 아이템은 /item)"
                        ))
                        .await;
                };
                let count = count.trim().parse::<i32>().unwrap_or(1).clamp(1, 64 * 36);
                let given = self.give_many(Item::new(id, 1), count).await?;
                let msg = format!("{} {given}개를 받았습니다.", items::name(id));
                Ok(self.message(&text::colored(&msg, "green")).await?)
            }
            "item" => match args.first().copied() {
                Some("list") | None => {
                    for d in custom_items::all() {
                        let line = format!(
                            "{} — {} ({})",
                            d.id,
                            d.name,
                            d.lore.first().cloned().unwrap_or_default()
                        );
                        self.message(&text::colored(&line, &d.color)).await?;
                    }
                    Ok(())
                }
                Some("reload") => {
                    let dir = self.server.data_dir.clone();
                    match custom_items::load(dir.as_deref()) {
                        Ok(n) => {
                            self.server
                                .publish(Event::PermissionsChanged { uuid: None }); // 명령어 트리 다시
                            Ok(self
                                .message(&text::colored(
                                    &format!("사용자 정의 아이템 {n}개를 읽었습니다."),
                                    "green",
                                ))
                                .await?)
                        }
                        Err(e) => self.error(&e).await,
                    }
                }
                Some(id) => match custom_items::index_of(id) {
                    Some(index) => {
                        let def = custom_items::get(index).unwrap();
                        let item = Item {
                            id: def.base,
                            count: 1,
                            custom: Some(index),
                        };
                        self.give_many(item, args.get(1).and_then(|c| c.parse().ok()).unwrap_or(1))
                            .await?;
                        Ok(self
                            .message(&text::colored(
                                &format!("{}을(를) 받았습니다.", def.name),
                                &def.color,
                            ))
                            .await?)
                    }
                    None => {
                        self.error(&format!("모르는 사용자 정의 아이템: {id} (/item list)"))
                            .await
                    }
                },
            },
            "summon" => match MobKind::by_name(rest) {
                Some(kind) => {
                    // 바닐라처럼 명령어를 친 자리에
                    mobs::spawn(self.server, kind, self.dimension, self.position);
                    Ok(self
                        .message(&text::colored(
                            &format!("{}을(를) 불렀습니다.", kind.display_name()),
                            "green",
                        ))
                        .await?)
                }
                None => {
                    self.error("사용법: /summon <cow|pig|sheep|chicken|zombie|spider>")
                        .await
                }
            },
            "kill" => {
                let mode = self.gamemode;
                if !mode.takes_damage() {
                    self.set_gamemode(GameMode::Survival).await?;
                }
                self.damage(f32::MAX, "generic", None, "").await?;
                if !mode.takes_damage() {
                    self.gamemode = mode;
                }
                Ok(())
            }
            "spawn" => {
                let spawn = terrain::spawn_point(self.dimension, self.server.seed);
                self.teleport_and_load(spawn).await
            }
            "msg" => {
                let (target, content) = rest.split_once(' ').unwrap_or((rest, ""));
                self.whisper(target, &sanitize(content)).await
            }
            "me" => {
                let action = sanitize(rest);
                if !action.is_empty() {
                    self.server.publish(Event::Chat {
                        kind: ChatKind::Emote,
                        sender: self.profile.name.clone(),
                        content: action,
                    });
                }
                Ok(())
            }
            "tp" => self.teleport_to_player(rest).await,
            "gamemode" => match GameMode::from_name(rest) {
                Some(mode) => {
                    self.set_gamemode(mode).await?;
                    let msg = format!("게임 모드를 {}(으)로 바꿨습니다.", mode.display_name());
                    Ok(self.message(&text::colored(&msg, "green")).await?)
                }
                None => {
                    self.error("사용법: /gamemode <survival|creative|adventure|spectator> [이름]")
                        .await
                }
            },
            "time" => {
                let when = rest.strip_prefix("set ").map(str::trim);
                match TIMES.iter().find(|(n, _)| Some(*n) == when) {
                    Some((label, ticks)) => {
                        self.server.set_day_time(*ticks);
                        self.server.publish(Event::System(text::colored(
                            &format!("{}님이 시간을 {label}(으)로 바꿨습니다.", self.profile.name),
                            "gray",
                        )));
                        Ok(())
                    }
                    None => {
                        self.error("사용법: /time set <day|noon|night|midnight>")
                            .await
                    }
                }
            }
            other => {
                self.error(&format!("모르는 명령어: /{other} (/help 참고)"))
                    .await
            }
        }
    }

    /// 여러 개를 최대 개수씩 나눠 넣습니다. 넣은 개수.
    async fn give_many(&mut self, item: Item, count: i32) -> std::io::Result<i32> {
        let max = custom_items::max_stack(&item);
        let mut left = count.clamp(1, 64 * 36);
        let total = left;
        while left > 0 {
            let n = left.min(max);
            let taken = self.give(item.with_count(n)).await?;
            left -= taken;
            if taken < n {
                break;
            }
        }
        Ok(total - left)
    }

    /// `/server`: 목록 또는 이동.
    async fn server_command(&mut self, name: Option<&str>) -> Result<(), FrameError> {
        let Some(net) = self.server.network() else {
            return self.error("다른 인스턴스가 없습니다.").await;
        };
        let Some(name) = name else {
            for s in net.all() {
                let here = std::ptr::eq(s.as_ref(), self.server);
                let line = format!(
                    "{}{} — {}명",
                    s.name,
                    if here { " (지금 여기)" } else { "" },
                    s.online()
                );
                self.message(&text::colored(&line, if here { "green" } else { "gray" }))
                    .await?;
            }
            return Ok(());
        };
        match net.get(name) {
            Some(to) if std::ptr::eq(to.as_ref(), self.server) => {
                self.error(&format!("이미 {name}에 있습니다.")).await
            }
            Some(to) => {
                let node = format!("dangu.server.{name}");
                // 인스턴스마다 막고 싶으면 `-dangu.server.<이름>` 권한으로
                if !self.server.perms.has(self.profile.uuid, &node, 0) {
                    return self
                        .error(&format!("{name}에 들어갈 권한이 없습니다."))
                        .await;
                }
                self.message(&text::colored(&format!("{name}(으)로 이동합니다…"), "aqua"))
                    .await?;
                self.start_transfer(to).await
            }
            None => {
                self.error(&format!("{name} 인스턴스가 없습니다. (/server 로 목록)"))
                    .await
            }
        }
    }

    async fn travel(&mut self, dimension: Dimension) -> Result<(), FrameError> {
        if dimension == self.dimension {
            let msg = format!("이미 {}에 있습니다.", dimension.display_name());
            return Ok(self.message(&text::colored(&msg, "gray")).await?);
        }
        self.change_dimension(dimension).await?;
        let msg = format!("{}(으)로 이동했습니다.", dimension.display_name());
        Ok(self.message(&text::colored(&msg, "green")).await?)
    }

    async fn whisper(&mut self, target: &str, content: &str) -> Result<(), FrameError> {
        if target.is_empty() || content.is_empty() {
            return self.error("사용법: /msg <이름> <내용>").await;
        }
        let Some(to) = self.server.find_player(target) else {
            return self
                .error(&format!("{target}님은 접속해 있지 않습니다."))
                .await;
        };
        self.server.publish(Event::Whisper {
            to: to.uuid,
            sender: self.profile.name.clone(),
            content: content.to_owned(),
        });
        // 자신에게 보낸 귓속말이면 받는 쪽 기록만으로 충분합니다.
        if to.uuid != self.profile.uuid {
            self.whisper_echo(&to.name, content).await?;
        }
        Ok(())
    }

    async fn teleport_to_player(&mut self, target: &str) -> Result<(), FrameError> {
        let Some(to) = self.server.find_player(target) else {
            return self
                .error(&format!("{target}님은 접속해 있지 않습니다."))
                .await;
        };
        if to.uuid == self.profile.uuid {
            return self.error("자기 자신에게는 갈 수 없습니다.").await;
        }
        if to.dimension != self.dimension {
            self.change_dimension(to.dimension).await?;
        }
        self.teleport_and_load(to.position).await?;
        let msg = format!("{}님에게 왔습니다.", to.name);
        Ok(self.message(&text::colored(&msg, "green")).await?)
    }

    async fn error(&mut self, message: &str) -> Result<(), FrameError> {
        Ok(self.message(&text::colored(message, "red")).await?)
    }
}
