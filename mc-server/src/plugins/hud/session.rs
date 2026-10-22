//! 접속한 사람 쪽: 1초마다 HUD를 다시 그려 보내고, `/hud`로 판을 끄고 켭니다.

use std::time::{Duration, Instant};

use tokio::io::AsyncWrite;

use super::Currencies;
use crate::packets::hud::{self, BossColor};
use crate::packets::play::GameMode;
use crate::plugins::rpg;
use crate::protocol::frame::FrameError;
use crate::protocol::nbt::Tag;
use crate::session::play::Session;
use crate::text;

/// HUD 보스바들의 UUID (연결마다 따로이므로 모두 같아도 됨): 이 값 + 줄 번호.
const BOSS_BAR: u128 = 0x6461_6e67_7548_5544_8000_0000_0000_0001;
/// 다른 기능이 액션바에 글을 띄우면 이만큼 HUD 아래쪽을 쉽니다.
const ACTION_BAR_PAUSE: Duration = Duration::from_secs(3);
/// 재화는 이만큼마다 다시 묻습니다 (담비 마스터에 묻는 것이라).
const CURRENCY_REFRESH: Duration = Duration::from_secs(2);
/// `hud_tick` 간격 (세션 루프의 `HUD_TICK`과 같음).
const HUD_SECS: f32 = 1.0;

#[derive(Default)]
pub struct HudState {
    /// `/hud`로 재화·스탯 판을 껐음.
    pub hidden: bool,
    /// 띄운 보스바 줄마다 마지막으로 보낸 제목.
    boss_bars: Vec<String>,
    currencies: Option<(i64, i64, i64)>,
    jobs: Vec<u32>,
    fetched: Option<Instant>,
    /// 다른 기능의 액션바 글을 가리지 않도록.
    pause_until: Option<Instant>,
}

impl HudState {
    /// 마지막으로 받은 직업 레벨들.
    pub fn job_levels(&self) -> Vec<u32> {
        self.jobs.clone()
    }

    /// 다른 기능이 액션바에 글을 띄웠음: HUD 아래쪽이 바로 덮지 않게 잠깐 쉽니다.
    pub fn pause_action_bar(&mut self) {
        self.pause_until = Some(Instant::now() + ACTION_BAR_PAUSE);
    }
}

impl<W: AsyncWrite + Unpin> Session<'_, W> {
    fn hud_active(&self) -> bool {
        super::enabled() && self.pack_loaded
    }

    pub(crate) fn rpg_stats(&self, job_levels: Vec<u32>) -> rpg::Stats {
        // 장비는 바닐라 갑옷 점수·무기 피해 대신 장비 능력치로
        let armor = [5, 6, 7, 8].map(|slot| {
            self.inventory[slot]
                .filter(|i| !self.is_gear(i))
                .map(|i| i.id)
        });
        let (gear, _, weapon) = self.equip_bonus();
        let held_damage = if weapon.is_some() {
            self.guild.bonus.damage
        } else {
            self.held_damage()
        };
        rpg::compute(&rpg::Input {
            job_levels,
            held_damage,
            armor,
            gear,
            sprinting: self.vitals.sprinting,
            sneaking: self.vitals.sneaking,
        })
    }

    /// 1초마다.
    pub(crate) async fn hud_tick(&mut self) -> Result<(), FrameError> {
        // 마나는 HUD가 없어도 찹니다
        let now_stats = self.rpg_stats(self.hud.jobs.clone());
        if !self.vitals.dead {
            self.mana.tick(HUD_SECS, now_stats.max_mana);
        }
        let power = now_stats.combat_power();
        self.guild_tick(power).await?;
        if !self.hud_active() {
            return Ok(());
        }
        let now = Instant::now();
        let mut job_levels = Vec::new();
        if !self.hud.hidden
            && self
                .hud
                .fetched
                .is_none_or(|t| now.duration_since(t) >= CURRENCY_REFRESH)
            && let Some(d) = crate::dambi::child::shared(self.profile.uuid).await
        {
            self.hud.currencies = Some((d.money, d.cash, d.afk));
            self.hud.fetched = Some(now);
            job_levels = d.jobs.iter().map(|j| j.level).collect();
            self.hud.jobs.clone_from(&job_levels);
        } else {
            job_levels.clone_from(&self.hud.jobs);
        }

        // 위: 재화·퀘스트 (바뀌었을 때만 다시)
        let lines = if self.hud.hidden {
            Vec::new()
        } else {
            let cur = self
                .hud
                .currencies
                .map(|(money, cash, afk)| Currencies { money, cash, afk });
            let quests = self.quest_hud();
            super::top(cur.as_ref(), Some(&quests))
        };
        self.update_boss_lines(&lines).await?;

        // 아래: 스탯 + 체력바 (액션바는 3초 뒤 사라지므로 매번)
        if self.hud.pause_until.is_some_and(|t| now < t) {
            return Ok(());
        }
        let full = self.rpg_stats(job_levels);
        let stats = (!self.hud.hidden).then_some(full);
        let survival = matches!(self.gamemode, GameMode::Survival | GameMode::Adventure);
        // 바닐라 체력(0..20)을 RPG 체력으로 보여 줌
        let hp = self.vitals.health / 20.0 * full.max_health;
        let line = super::bottom(&super::Bottom {
            stats: stats.as_ref(),
            health: survival.then_some((hp, full.max_health)),
            mana: survival.then_some((self.mana.current, self.mana.max)),
        });
        self.send(hud::SET_ACTION_BAR_TEXT_ID, &hud::encode_text(&line))
            .await?;
        Ok(())
    }

    /// 보스바 줄들을 `lines`에 맞춥니다. 줄 수가 바뀌면 순서가 섞이지 않게 모두 다시 띄웁니다.
    async fn update_boss_lines(&mut self, lines: &[Tag]) -> Result<(), FrameError> {
        let keys: Vec<String> = lines.iter().map(|t| format!("{t:?}")).collect();
        if keys == self.hud.boss_bars {
            return Ok(());
        }
        if keys.len() != self.hud.boss_bars.len() {
            for i in 0..self.hud.boss_bars.len() {
                self.send(
                    hud::BOSS_EVENT_ID,
                    &hud::encode_boss_remove(BOSS_BAR + i as u128),
                )
                .await?;
            }
            for (i, t) in lines.iter().enumerate() {
                let body = hud::encode_boss_add(BOSS_BAR + i as u128, t, 0.0, BossColor::Pink);
                self.send(hud::BOSS_EVENT_ID, &body).await?;
            }
        } else {
            for (i, t) in lines.iter().enumerate() {
                if self.hud.boss_bars[i] != keys[i] {
                    let body = hud::encode_boss_title(BOSS_BAR + i as u128, t);
                    self.send(hud::BOSS_EVENT_ID, &body).await?;
                }
            }
        }
        self.hud.boss_bars = keys;
        Ok(())
    }

    pub(crate) async fn hud_command(&mut self) -> Result<(), FrameError> {
        if !self.hud_active() {
            return self
                .error("HUD는 서버 리소스 팩을 받은 사람에게만 보입니다.")
                .await;
        }
        self.hud.hidden = !self.hud.hidden;
        let msg = if self.hud.hidden {
            "재화·퀘스트·스탯 판을 껐습니다. (/hud 로 다시 켜기)"
        } else {
            "재화·퀘스트·스탯 판을 켰습니다."
        };
        self.message(&text::colored(msg, "green")).await?;
        self.hud_tick().await
    }
}
