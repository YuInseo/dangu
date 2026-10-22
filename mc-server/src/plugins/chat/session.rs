//! 접속한 사람 쪽: 채팅 칠 때 채널 고르기, `/채널` 명령어, 받은 말 보여 주기.
//!
//! ```text
//! /채널                 지금 채널과 목록 (끈 채널은 회색)
//! /채널 <이름>          기본 채널 바꾸기 (전체·채널·주변·거래·길드·관리자)
//! /채널 끄기 <이름>     그 채널 안 받기 (자기 말은 보임)   /채널 켜기 <이름>
//! ```

use std::time::Instant;

use tokio::io::AsyncWrite;

use super::{Channel, Message, Receiver};
use crate::protocol::frame::FrameError;
use crate::session::play::Session;
use crate::text;

impl<W: AsyncWrite + Unpin> Session<'_, W> {
    fn staff(&self) -> bool {
        self.server
            .perms
            .has(self.profile.uuid, super::STAFF_NODE, 2)
    }

    fn guild_name(&self) -> Option<String> {
        crate::plugins::guild::name_of(self.profile.uuid)
    }

    /// 채팅 한 줄을 채널로 보냅니다 (앞 기호가 있으면 그 채널로 한 번만).
    pub(crate) async fn channel_chat(&mut self, line: &str) -> Result<(), FrameError> {
        let prefs = super::prefs(self.profile.uuid);
        let (channel, content) = Channel::split_prefix(line).unwrap_or((prefs.current(), line));
        if channel == Channel::Staff && !self.staff() {
            return self.error("관리자 채널은 관리자만 쓸 수 있습니다.").await;
        }
        let guild = if channel == Channel::Guild {
            match self.guild_name() {
                Some(g) => Some(g),
                None => return self.error("길드에 들어가 있지 않습니다.").await,
            }
        } else {
            None
        };
        if let Some(left) = super::throttle(self.profile.uuid, channel, Instant::now()) {
            let msg = format!(
                "{} 채널은 {:.0}초 뒤에 다시 말할 수 있습니다.",
                channel.name(),
                left.as_secs_f64().ceil()
            );
            return self.error(&msg).await;
        }
        let m = Message {
            channel,
            sender: self.profile.uuid,
            sender_name: self.profile.name.clone(),
            instance: self.server.name.clone(),
            dimension: self.dimension,
            position: self.position,
            guild,
            content: crate::plugins::contents::emojify(content),
        };
        println!("[{}] <{}> {}", channel.name(), m.sender_name, m.content);
        super::send(self.server, m);
        Ok(())
    }

    /// 다른 사람(또는 자신)이 보낸 채널 채팅이 왔을 때.
    pub(crate) async fn deliver_channel_chat(&mut self, m: &Message) -> std::io::Result<()> {
        let prefs = super::prefs(self.profile.uuid);
        let guild = if m.channel == super::Channel::Guild {
            self.guild_name()
        } else {
            None
        };
        let me = Receiver {
            uuid: self.profile.uuid,
            instance: &self.server.name,
            dimension: self.dimension,
            position: self.position,
            staff: m.channel == Channel::Staff && self.staff(),
            guild: guild.as_deref(),
            muted: &prefs.muted,
        };
        if super::reaches(m, &me) {
            self.message(&super::render(m)).await?;
        }
        Ok(())
    }

    pub(crate) async fn channel_command(&mut self, args: &[&str]) -> Result<(), FrameError> {
        let uuid = self.profile.uuid;
        match args {
            [] | ["목록"] | ["list"] => {
                let p = super::prefs(uuid);
                self.message(&text::colored(
                    &format!("지금 채널: {}", p.current().name()),
                    "gold",
                ))
                .await?;
                for c in Channel::ALL {
                    if c == Channel::Staff && !self.staff() {
                        continue;
                    }
                    let (head, _) = c.colors();
                    let off = p.muted.contains(&c);
                    let line = format!(
                        " {} {}  — 앞에 {} 를 붙이면 한 번만{}",
                        if c == p.current() { "▶" } else { " " },
                        c.name(),
                        c.prefix(),
                        if off { " (꺼 둠)" } else { "" }
                    );
                    let tag = text::colored(&line, if off { "dark_gray" } else { head });
                    let tag = text::interactive(
                        tag,
                        Some("누르면 이 채널로"),
                        Some(&format!("/채널 {}", c.name())),
                    );
                    self.message(&tag).await?;
                }
                Ok(())
            }
            [onoff @ ("끄기" | "켜기" | "mute" | "unmute"), name] => {
                let Some(c) = Channel::parse(name) else {
                    return self.error(&format!("모르는 채널: {name}")).await;
                };
                let mute = matches!(*onoff, "끄기" | "mute");
                if mute && c == super::prefs(uuid).current() {
                    return self
                        .error("지금 쓰는 채널은 끌 수 없습니다. 다른 채널로 바꾼 뒤 끄세요.")
                        .await;
                }
                super::update_prefs(uuid, |p| {
                    if mute {
                        p.muted.insert(c);
                    } else {
                        p.muted.remove(&c);
                    }
                });
                let msg = format!(
                    "{} 채널을 {}.",
                    c.name(),
                    if mute { "껐습니다" } else { "켰습니다" }
                );
                Ok(self.message(&text::colored(&msg, "green")).await?)
            }
            [name] => {
                let Some(c) = Channel::parse(name) else {
                    return self
                        .error(&format!("모르는 채널: {name} (/채널 로 목록)"))
                        .await;
                };
                if c == Channel::Staff && !self.staff() {
                    return self.error("관리자 채널은 관리자만 쓸 수 있습니다.").await;
                }
                if c == Channel::Guild && self.guild_name().is_none() {
                    return self.error("길드에 들어가 있지 않습니다.").await;
                }
                super::update_prefs(uuid, |p| {
                    p.channel = Some(c);
                    p.muted.remove(&c);
                });
                let (head, _) = c.colors();
                let msg = format!("이제 채팅이 [{}] 채널로 갑니다.", c.name());
                Ok(self.message(&text::colored(&msg, head)).await?)
            }
            _ => {
                self.error("사용법: /채널 [이름] | /채널 끄기|켜기 <이름>")
                    .await
            }
        }
    }
}
