//! 명령어: 문법 트리(클라이언트 자동 완성용)와 실행.

use tokio::io::AsyncWrite;

use super::play::{Session, sanitize};
use crate::data::argument_type;
use crate::packets::commands::{CommandTree, ROOT};
use crate::packets::play::{ChatKind, GameMode};
use crate::protocol::frame::FrameError;
use crate::server::Event;
use crate::text;
use crate::world::{Dimension, terrain};

/// `/time set <이름>`이 받는 시각.
const TIMES: &[(&str, i64)] = &[
    ("day", 1000),
    ("noon", 6000),
    ("night", 13000),
    ("midnight", 18000),
];

/// 클라이언트에 보내는 명령어 트리. [`Session::command`]가 받는 것과 같아야 합니다.
pub fn tree() -> CommandTree {
    let mut t = CommandTree::default();
    for name in ["overworld", "nether", "end", "spawn", "help", "list"] {
        t.literal(ROOT, name, true);
    }
    // /msg <대상> <내용>, 별칭 /tell /w
    for name in ["msg", "tell", "w"] {
        let cmd = t.literal(ROOT, name, false);
        let target = t.argument(cmd, "targets", argument_type::GAME_PROFILE, false);
        t.argument(target, "message", argument_type::MESSAGE, true);
    }
    let me = t.literal(ROOT, "me", false);
    t.argument(me, "action", argument_type::MESSAGE, true);
    let tp = t.literal(ROOT, "tp", false);
    t.argument(tp, "target", argument_type::GAME_PROFILE, true);
    let gm = t.literal(ROOT, "gamemode", false);
    for mode in GameMode::ALL {
        t.literal(gm, mode.name(), true);
    }
    let time = t.literal(ROOT, "time", false);
    let set = t.literal(time, "set", false);
    for (name, _) in TIMES {
        t.literal(set, name, true);
    }
    t
}

const HELP: &[&str] = &[
    "/overworld, /nether, /end — 차원 이동",
    "/spawn — 이 차원의 스폰으로",
    "/msg <이름> <내용> — 귓속말 (/tell, /w)",
    "/me <행동> — 행동 알리기",
    "/tp <이름> — 그 사람에게 가기",
    "/gamemode <survival|creative|adventure|spectator>",
    "/time set <day|noon|night|midnight>",
    "/list — 접속자 목록",
];

impl<W: AsyncWrite + Unpin> Session<'_, W> {
    pub(super) async fn command(&mut self, line: &str) -> Result<(), FrameError> {
        let (name, rest) = line.split_once(' ').unwrap_or((line, ""));
        let rest = rest.trim();

        if let Some(dimension) = Dimension::from_command(name) {
            return self.travel(dimension).await;
        }
        match name {
            "spawn" => {
                let spawn = terrain::spawn_point(self.dimension, self.server.seed);
                self.teleport_and_load(spawn).await
            }
            "help" => {
                for line in HELP {
                    self.message(&text::plain(line)).await?;
                }
                Ok(())
            }
            "list" => {
                let names: Vec<_> = self.server.players().into_iter().map(|p| p.name).collect();
                let msg = format!("접속자 {}명: {}", names.len(), names.join(", "));
                Ok(self.message(&text::plain(&msg)).await?)
            }
            "msg" | "tell" | "w" => {
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
                    self.error("사용법: /gamemode <survival|creative|adventure|spectator>")
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
