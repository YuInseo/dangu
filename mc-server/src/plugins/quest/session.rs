//! 접속한 사람 쪽: 일어난 일 세기, 보상 주기, `/퀘스트`, HUD에 보일 퀘스트.

use std::sync::OnceLock;

use tokio::io::AsyncWrite;

use super::config;
use super::progress::{self, Completed, Happened, HudQuest, Kind, Progress};
use crate::packets::world::Item;
use crate::plugins::store::{JsonFile, data_path};
use crate::protocol::frame::FrameError;
use crate::session::play::Session;
use crate::text;

static PROGRESS: OnceLock<JsonFile<Progress>> = OnceLock::new();

/// 모든 인스턴스가 같이 쓰는 진행 파일.
pub fn progress() -> &'static JsonFile<Progress> {
    PROGRESS.get_or_init(|| JsonFile::in_memory(progress::encode, progress::decode))
}

/// 서버를 켤 때 한 번: 퀘스트 설정과 진행 파일.
pub fn open_global() {
    config::open_global();
    let _ = PROGRESS.set(JsonFile::open(
        data_path("MC_QUEST_PROGRESS", "quest_progress.json"),
        progress::encode,
        progress::decode,
    ));
}

/// 아이템 이름(`bread`, `ns:id`) → 아이템.
pub fn item_by_name(name: &str, count: i32) -> Option<Item> {
    crate::plugins::contents::item(name, count).or_else(|| {
        let id = crate::data::items::by_name(name.strip_prefix("minecraft:").unwrap_or(name))?;
        (id > 0).then(|| Item::new(id, count))
    })
}

/// 이 아이템이 이 이름(별표 가능)에 맞나.
pub fn item_is(item: &Item, pattern: &str) -> bool {
    match item.custom.and_then(crate::game::custom_items::get) {
        Some(c) => super::name_matches(pattern, &c.id),
        None => super::name_matches(pattern, crate::data::items::name(item.id)),
    }
}

impl<W: AsyncWrite + Unpin> Session<'_, W> {
    fn my_guild_id(&self) -> Option<u32> {
        crate::plugins::guild::guilds().read(|d| d.of(self.profile.uuid).map(|g| g.id))
    }

    /// 게임에서 일어난 일 (캐기·잡기 …).
    pub(crate) async fn quest_happened(&mut self, h: Happened) -> std::io::Result<()> {
        let guild = self.my_guild_id();
        let uuid = self.profile.uuid;
        let done =
            config::with(|cfg| progress().write(|p| p.record(uuid, guild, &h, cfg, super::now())));
        for c in done {
            self.grant_quest(c).await?;
        }
        Ok(())
    }

    /// 끝난 퀘스트의 보상.
    pub(crate) async fn grant_quest(&mut self, c: Completed) -> std::io::Result<()> {
        use crate::dambi::child;
        let me = self.profile.uuid;
        let r = &c.def.rewards;
        if r.money != 0 {
            child::shared_add(me, "money", r.money).await;
        }
        if r.cash != 0 {
            child::shared_add(me, "cash", r.cash).await;
        }
        for (job, xp) in &r.job_xp {
            crate::plugins::rank::job_xp(me, job, *xp).await;
        }
        for (name, count) in &r.items {
            if let Some(item) = item_by_name(name, 1) {
                let given = self.give_many(item, *count).await?;
                if given < *count {
                    self.throw(item.with_count(*count - given));
                }
            }
        }
        let mut parts = Vec::new();
        if r.money > 0 {
            parts.push(format!("돈 {}", crate::plugins::hud::number(r.money)));
        }
        if r.cash > 0 {
            parts.push(format!("캐시 {}", r.cash));
        }
        for (name, count) in &r.items {
            let label = item_by_name(name, 1)
                .and_then(|i| {
                    i.custom
                        .and_then(crate::game::custom_items::get)
                        .map(|c| c.name)
                })
                .unwrap_or_else(|| name.clone());
            parts.push(format!("{label} ×{count}"));
        }
        if let (Kind::Guild, Some(gid)) = (c.kind, c.guild) {
            crate::plugins::guild::guilds().write(|d| d.add_gp(gid, Some(me), r.gp));
            self.tell_guild(&format!(
                "[길드 주간] {} 완료! 길드 GP +{} (마지막: {})",
                c.def.name, r.gp, self.profile.name
            ));
        }
        let line = format!(
            "[{}] {} 완료!{}",
            c.kind.name(),
            c.def.name,
            if parts.is_empty() {
                String::new()
            } else {
                format!(" 보상: {}", parts.join(", "))
            }
        );
        self.message(&text::colored(&line, "gold")).await?;
        self.show_title(&format!("퀘스트 완료|{}", c.def.name))
            .await?;
        Ok(())
    }

    /// HUD 오른쪽 위에 보일 퀘스트.
    pub(crate) fn quest_hud(&self) -> Vec<HudQuest> {
        let guild = self.my_guild_id();
        let uuid = self.profile.uuid;
        config::with(|cfg| progress().read(|p| p.hud(uuid, guild, cfg, super::now())))
    }

    pub(crate) async fn quest_command(&mut self, args: &[&str]) -> Result<(), FrameError> {
        let tab = match args.first().copied() {
            Some("주간" | "weekly") => Kind::Weekly,
            Some("길드" | "guild") => Kind::Guild,
            _ => Kind::Normal,
        };
        Ok(self.open_quests(super::gui::Page { tab, page: 0 }).await?)
    }
}
