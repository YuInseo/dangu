//! `/npc` (= `/엔피시`): NPC 만들기·꾸미기, 그리고 NPC를 우클릭했을 때.
//!
//! ```text
//! /npc create <이름>          (생성)   지금 자리에, 플레이어 모습으로. 만든 것이 선택됨
//! /npc select [번호]          (선택)   번호가 없으면 6칸 안의 가장 가까운 것
//! /npc list | info            (목록 | 정보)
//! /npc rename <이름>          (이름)   &a 같은 색 코드
//! /npc type <player|몹 이름>   (타입)
//! /npc skin <플레이어>        (스킨)   접속 중이면 그 사람 스킨, 아니면 Mojang에서
//! /npc desc <글|clear>        (설명)   이름 아래 한 줄 (플레이어 모습만)
//! /npc tphere                 (여기로)
//! /npc look                   (시선)   가까운 사람 쳐다보기 켜기/끄기
//! /npc msg add <글> | clear   (메시지)
//! /npc cmd add [console] <명령어> | clear | list   (명령)  {player} = 누른 사람
//! /npc action <lottery|server:<인스턴스>|none>       (동작)
//! /npc remove [번호]          (삭제)
//! ```

use tokio::io::AsyncWrite;

use crate::plugins::npc::{self, Npc, NpcCommand};
use crate::protocol::frame::FrameError;
use crate::session::play::Session;
use crate::text;

fn sub(word: &str) -> &str {
    match word {
        "생성" | "만들기" => "create",
        "선택" => "select",
        "목록" => "list",
        "정보" => "info",
        "이름" => "rename",
        "타입" | "종류" => "type",
        "스킨" => "skin",
        "설명" => "desc",
        "여기로" | "이동" => "tphere",
        "시선" => "look",
        "메시지" => "msg",
        "명령" | "명령어" => "cmd",
        "동작" => "action",
        "삭제" | "제거" => "remove",
        "추가" => "add",
        "비우기" | "지우기" => "clear",
        "콘솔" => "console",
        w => w,
    }
}

impl<W: AsyncWrite + Unpin> Session<'_, W> {
    async fn say(&mut self, line: &str, color: &str) -> Result<(), FrameError> {
        Ok(self.message(&text::colored(line, color)).await?)
    }

    /// NPC를 우클릭: 메시지 → 동작 → 명령어.
    pub(crate) async fn npc_clicked(&mut self, id: u32) -> Result<(), FrameError> {
        let Some(npc) = self.server.npcs.get(id) else {
            return Ok(());
        };
        let (messages, action, commands) = npc::on_click(&npc, &self.profile.name);
        for m in messages {
            let line = format!("{}&r: {}", npc.name, m);
            self.message(&text::plain(&text::legacy_colors(&line)))
                .await?;
        }
        match action.as_deref() {
            Some("lottery") => {
                self.open_lottery(crate::plugins::lottery::gui::Page::Hub)
                    .await?
            }
            Some(a) if a.starts_with("server:") => {
                let to = a["server:".len()..].to_owned();
                self.move_command(Some(&to)).await?;
            }
            _ => {}
        }
        for c in commands {
            let line = c.line.trim_start_matches('/').to_owned();
            if c.console {
                let mut words = line.split_whitespace();
                let name = words.next().unwrap_or_default().to_owned();
                let args: Vec<&str> = words.collect();
                let actor = crate::admin::Actor::console();
                if crate::admin::run(self.server, &actor, &name, &args).is_none() {
                    eprintln!("NPC {id}: 콘솔로 실행할 수 없는 명령어 /{line}");
                }
            } else {
                Box::pin(self.command(&line)).await?;
            }
        }
        Ok(())
    }

    fn selected(&self) -> Option<Npc> {
        self.selected_npc.and_then(|id| self.server.npcs.get(id))
    }

    pub(crate) async fn npc_command(&mut self, args: &[&str]) -> Result<(), FrameError> {
        let words: Vec<&str> = args.iter().map(|w| sub(w)).collect();
        let rest = |from: usize| args.get(from..).map(|a| a.join(" ")).unwrap_or_default();
        let npcs = &self.server.npcs;
        match words.as_slice() {
            [] | ["help"] => {
                for l in [
                    "/npc create <이름> · select [번호] · list · info · remove [번호]",
                    "/npc rename <이름> · type <player|몹> · skin <플레이어> · desc <글|clear>",
                    "/npc tphere · look · msg add <글>|clear · cmd add [console] <명령어>|clear|list",
                    "/npc action <lottery|server:<인스턴스>|none>",
                ] {
                    self.say(l, "gray").await?;
                }
                Ok(())
            }
            ["create", ..] if args.len() > 1 => {
                let n = Npc::new(
                    0,
                    &rest(1),
                    "player",
                    self.dimension,
                    self.position,
                    self.yaw,
                );
                let id = npcs.create(self.server, n);
                self.selected_npc = Some(id);
                self.say(&format!("NPC #{id}을(를) 만들고 선택했습니다."), "green")
                    .await
            }
            ["list"] => {
                let all = npcs.all();
                if all.is_empty() {
                    return self.say("NPC가 없습니다.", "gray").await;
                }
                for n in all {
                    let line = format!(
                        "#{} {} — {} ({:.0}, {:.0}, {:.0}){}",
                        n.id,
                        n.name,
                        n.kind,
                        n.pos.0,
                        n.pos.1,
                        n.pos.2,
                        n.action.map(|a| format!(" [{a}]")).unwrap_or_default()
                    );
                    self.message(&text::plain(&text::legacy_colors(&line)))
                        .await?;
                }
                Ok(())
            }
            ["select"] => match npcs.nearest(self.dimension, self.position, 6.0) {
                Some(id) => {
                    self.selected_npc = Some(id);
                    self.say(&format!("NPC #{id} 선택"), "green").await
                }
                None => self.error("6칸 안에 NPC가 없습니다.").await,
            },
            ["select", id] | ["remove", id] if id.parse::<u32>().is_ok() => {
                let id: u32 = id.parse().unwrap();
                if words[0] == "remove" {
                    if npcs.remove(self.server, id) {
                        self.say(&format!("NPC #{id}을(를) 지웠습니다."), "green")
                            .await
                    } else {
                        self.error(&format!("NPC #{id}이(가) 없습니다.")).await
                    }
                } else if npcs.get(id).is_some() {
                    self.selected_npc = Some(id);
                    self.say(&format!("NPC #{id} 선택"), "green").await
                } else {
                    self.error(&format!("NPC #{id}이(가) 없습니다.")).await
                }
            }
            _ => {
                let Some(n) = self.selected() else {
                    return self
                        .error("먼저 /npc select [번호]로 NPC를 고르세요.")
                        .await;
                };
                self.edit_selected(n, &words, args).await
            }
        }
    }

    async fn edit_selected(
        &mut self,
        n: Npc,
        words: &[&str],
        args: &[&str],
    ) -> Result<(), FrameError> {
        let id = n.id;
        let rest = |from: usize| args.get(from..).map(|a| a.join(" ")).unwrap_or_default();
        let server = self.server;
        let edit = |f: &mut dyn FnMut(&mut Npc)| server.npcs.edit(server, id, |n| f(n));
        match words {
            ["info"] => {
                let lines = [
                    format!("#{} {} ({})", n.id, n.name, n.kind),
                    format!(
                        "위치 {} ({:.1}, {:.1}, {:.1}) · 시선 {}",
                        n.dimension.display_name(),
                        n.pos.0,
                        n.pos.1,
                        n.pos.2,
                        if n.look_close { "켬" } else { "끔" }
                    ),
                    format!(
                        "스킨 {} · 동작 {}",
                        n.skin.as_ref().map_or("기본", |s| s.0.as_str()),
                        n.action.as_deref().unwrap_or("없음")
                    ),
                ];
                for l in lines {
                    self.message(&text::plain(&text::legacy_colors(&l))).await?;
                }
                for m in &n.messages {
                    self.say(&format!("  메시지: {m}"), "gray").await?;
                }
                for c in &n.commands {
                    let who = if c.console { "콘솔" } else { "플레이어" };
                    self.say(&format!("  명령({who}): /{}", c.line), "gray")
                        .await?;
                }
                Ok(())
            }
            ["remove"] => {
                self.server.npcs.remove(self.server, id);
                self.selected_npc = None;
                self.say(&format!("NPC #{id}을(를) 지웠습니다."), "green")
                    .await
            }
            ["rename", ..] if args.len() > 1 => {
                let name = rest(1);
                edit(&mut |n| n.name = name.clone());
                self.say("이름을 바꿨습니다.", "green").await
            }
            ["type", kind] => {
                let kind = kind.strip_prefix("minecraft:").unwrap_or(kind).to_owned();
                if kind != "player" && crate::data::entity_id(&kind).is_none() {
                    return self.error(&format!("모르는 종류: {kind}")).await;
                }
                edit(&mut |n| n.kind = kind.clone());
                self.say("모습을 바꿨습니다.", "green").await
            }
            ["skin", name] => {
                let name = name.to_string();
                let props = match self.server.find_player_anywhere(&name) {
                    Some(p) if !p.properties.is_empty() => Some(p.properties.to_vec()),
                    _ => match self.server.sessions.profile_by_name(&name).await {
                        Ok(Some(profile)) => Some(profile.properties),
                        Ok(None) => None,
                        Err(e) => {
                            return self.error(&format!("스킨을 받지 못했습니다: {e}")).await;
                        }
                    },
                };
                let Some(props) = props else {
                    return self
                        .error(&format!("{name}의 스킨을 찾지 못했습니다."))
                        .await;
                };
                edit(&mut |n| {
                    n.skin = Some((name.clone(), props.clone()));
                    n.kind = "player".into();
                });
                self.say(&format!("{name}의 스킨을 입혔습니다."), "green")
                    .await
            }
            ["desc", "clear"] => {
                edit(&mut |n| n.description = None);
                self.say("설명을 지웠습니다.", "green").await
            }
            ["desc", ..] if args.len() > 1 => {
                let d = rest(1);
                edit(&mut |n| n.description = Some(d.clone()));
                self.say("설명을 바꿨습니다.", "green").await
            }
            ["tphere"] => {
                let (pos, yaw, dim) = (self.position, self.yaw, self.dimension);
                edit(&mut |n| {
                    n.pos = pos;
                    n.yaw = yaw;
                    n.dimension = dim;
                });
                self.say("이 자리로 옮겼습니다.", "green").await
            }
            ["look"] => {
                let on = !n.look_close;
                edit(&mut |n| n.look_close = on);
                self.say(
                    if on {
                        "가까운 사람을 쳐다봅니다."
                    } else {
                        "쳐다보지 않습니다."
                    },
                    "green",
                )
                .await
            }
            ["msg", "add", ..] if args.len() > 2 => {
                let m = rest(2);
                edit(&mut |n| n.messages.push(m.clone()));
                self.say("메시지를 더했습니다.", "green").await
            }
            ["msg", "clear"] => {
                edit(&mut |n| n.messages.clear());
                self.say("메시지를 모두 지웠습니다.", "green").await
            }
            ["cmd", "add", "console", ..] if args.len() > 3 => {
                let line = rest(3);
                edit(&mut |n| {
                    n.commands.push(NpcCommand {
                        console: true,
                        line: line.clone(),
                    })
                });
                self.say("콘솔 명령어를 더했습니다.", "green").await
            }
            ["cmd", "add", ..] if args.len() > 2 => {
                let line = rest(2);
                edit(&mut |n| {
                    n.commands.push(NpcCommand {
                        console: false,
                        line: line.clone(),
                    })
                });
                self.say("명령어를 더했습니다 (누른 사람의 권한으로 실행).", "green")
                    .await
            }
            ["cmd", "clear"] => {
                edit(&mut |n| n.commands.clear());
                self.say("명령어를 모두 지웠습니다.", "green").await
            }
            ["cmd", "list"] | ["cmd"] => {
                for c in &n.commands {
                    let who = if c.console { "콘솔" } else { "플레이어" };
                    self.say(&format!("({who}) /{}", c.line), "gray").await?;
                }
                Ok(())
            }
            ["action", "none"] | ["action", "없음"] => {
                edit(&mut |n| n.action = None);
                self.say("동작을 지웠습니다.", "green").await
            }
            ["action", a] if *a == "lottery" || a.starts_with("server:") => {
                let a = a.to_string();
                edit(&mut |n| n.action = Some(a.clone()));
                self.say("동작을 정했습니다.", "green").await
            }
            _ => self.error("모르는 /npc 명령입니다 (/npc help).").await,
        }
    }
}
