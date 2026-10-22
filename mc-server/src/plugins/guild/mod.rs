//! 길드 (마을 서버 기획): 정보 · 창고 · 땅 · 업그레이드(신호기) · 퀘스트 · 설정·권한.
//!
//! | 기획 | 여기서 |
//! |---|---|
//! | 길드 정보: 이름·레벨·길드장·인원·전투력 | [`Guild`], 전투력 = 길드원 전투력 합 ([`crate::plugins::rpg`]) |
//! | 길드 창고 (페이지) + 기록 | [`Guild::storage`] 페이지마다 45칸, [`Guild::log`] |
//! | 땅: 좌표 · 좌클릭 이동 · Shift+좌클릭 판매(확인창) | [`land`], 청크 단위, 보호 |
//! | 레벨별 효과 (인원·땅·창고·신호기 칸·보너스) — 웹에서 고침 | [`config`] |
//! | 업그레이드: 땅·인원·창고 (레벨 효과 위에 더함) | [`Upgrade`] |
//! | 작물 성장: 청크마다 촉진제로 단계 올림, 길드 청크 지도 창 | [`land`], [`gui`] |
//! | 신호기: 범위 없음·무한 지속, n번째 효과 = 신호기 n개, 단계 올리기 = 광물 + 돈 | [`beacon`] |
//! | 퀘스트: 길드 주간 | [`crate::plugins::quest`] |
//! | 2배 이벤트: 생활 직업 경험치 · 높은 등급 재료 확률 | [`GuildEvent`] |
//! | 역할: 길드장 · 부길드장(2명까지, 길드장이 허락한 권한만) · 길드원 | [`Role`], [`Perm`] |
//! | 해산(물어보기) · 설명 · 방문객 · 길드원·역할 관리 · 창고 기록 · 비공개 · 가입 신청 | [`gui`] |
//!
//! 기획의 "미정: GP·공헌도·경험치·명예훈장"은 하나로: 퀘스트가 주는 **GP**가 길드 경험치(레벨,
//! 줄지 않음)와 쓸 수 있는 GP(이벤트에 씀)로 함께 쌓이고, 해낸 사람의 **공헌도**에도 더해집니다.
//!
//! 저장: [`store`] (모든 인스턴스가 같은 파일, 다른 프로세스와는 잠금 파일로 차례를 지킴).

pub mod art;
pub mod beacon;
pub mod config;
pub mod gui;
pub mod land;
mod map;
pub(crate) mod session;
pub mod store;

use crate::packets::world::Item;
use crate::world::Dimension;

pub use land::growth_at;
pub use session::{BUS, GuildState, name_of, notify, receive as session_receive};
pub use store::{Data, guilds};

pub type GuildId = u32;

/// 창고 한 페이지 칸 수 (6줄 창의 위 5줄).
pub const PAGE_SLOTS: usize = 45;
/// 창고 기록은 이만큼만 남깁니다.
pub const LOG_KEEP: usize = 300;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Role {
    Member,
    Officer,
    Leader,
}

impl Role {
    pub fn name(self) -> &'static str {
        match self {
            Role::Leader => "길드장",
            Role::Officer => "부길드장",
            Role::Member => "길드원",
        }
    }
    fn key(self) -> &'static str {
        match self {
            Role::Leader => "leader",
            Role::Officer => "officer",
            Role::Member => "member",
        }
    }
    fn parse(s: &str) -> Option<Role> {
        [Role::Leader, Role::Officer, Role::Member]
            .into_iter()
            .find(|r| r.key() == s)
    }
}

/// 길드장이 부길드장·길드원에게 줄 수 있는 권한. 길드장 위임·부길드장 임명·역할 관리·해산은
/// 길드장만 (권한으로 줄 수 없음).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Perm {
    Kick,
    Description,
    Events,
    StorageUse,
    StorageLog,
    Spawn,
    EnterMessage,
    LeaveMessage,
    AcceptRequests,
    Land,
    Upgrade,
    Visitors,
    Privacy,
}

impl Perm {
    pub const ALL: [Perm; 13] = [
        Perm::Kick,
        Perm::Description,
        Perm::Events,
        Perm::StorageUse,
        Perm::StorageLog,
        Perm::Spawn,
        Perm::EnterMessage,
        Perm::LeaveMessage,
        Perm::AcceptRequests,
        Perm::Land,
        Perm::Upgrade,
        Perm::Visitors,
        Perm::Privacy,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Perm::Kick => "길드원 강퇴",
            Perm::Description => "길드 설명",
            Perm::Events => "2배 이벤트 설정",
            Perm::StorageUse => "창고 넣고 빼기",
            Perm::StorageLog => "창고 기록 보기",
            Perm::Spawn => "스폰 지정",
            Perm::EnterMessage => "입장 메시지 설정",
            Perm::LeaveMessage => "퇴장 메시지 설정",
            Perm::AcceptRequests => "가입 신청 수락·거절",
            Perm::Land => "땅 사고팔기",
            Perm::Upgrade => "업그레이드·신호기",
            Perm::Visitors => "방문객 관리",
            Perm::Privacy => "비공개 설정",
        }
    }

    pub fn bit(self) -> u32 {
        1 << Perm::ALL.iter().position(|p| *p == self).unwrap()
    }

    /// 처음 만든 길드의 기본값.
    pub fn defaults(role: Role) -> u32 {
        let list: &[Perm] = match role {
            Role::Leader => &Perm::ALL,
            Role::Officer => &[
                Perm::Description,
                Perm::StorageUse,
                Perm::StorageLog,
                Perm::AcceptRequests,
            ],
            Role::Member => &[Perm::StorageUse],
        };
        list.iter().fold(0, |m, p| m | p.bit())
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Member {
    pub uuid: u128,
    pub name: String,
    pub role: Role,
    pub joined: i64,
    /// 공헌도 (퀘스트로 쌓은 GP).
    pub contribution: i64,
    /// 마지막으로 본 전투력.
    pub power: i64,
}

/// 위치 하나 (인스턴스까지).
#[derive(Debug, Clone, PartialEq)]
pub struct Spot {
    pub instance: String,
    pub dimension: Dimension,
    pub pos: (f64, f64, f64),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Land {
    pub instance: String,
    pub dimension: Dimension,
    pub chunk: (i32, i32),
    pub bought: i64,
    /// 산 값 (팔 때 일부 돌려받음).
    pub paid: i64,
    /// 이 청크의 작물 성장 단계 (작물 촉진제로 올림, [`config::Config::crop_levels`]).
    pub crop_level: u8,
}

/// 돈으로 사는 업그레이드 세 가지 (값·효과는 [`config`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Upgrade {
    Land,
    Members,
    Storage,
}

impl Upgrade {
    pub const ALL: [Upgrade; 3] = [Upgrade::Land, Upgrade::Members, Upgrade::Storage];

    pub fn name(self) -> &'static str {
        match self {
            Upgrade::Land => "땅 늘리기",
            Upgrade::Members => "인원 늘리기",
            Upgrade::Storage => "창고 늘리기",
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            Upgrade::Land => "land",
            Upgrade::Members => "members",
            Upgrade::Storage => "storage",
        }
    }

    pub fn rule(self) -> config::UpgradeRule {
        config::with(|c| c.upgrade(self.key()).cloned()).unwrap_or(config::UpgradeRule {
            kind: self.key().into(),
            max_level: 0,
            per_level: 0,
            base_cost: 0,
            required_level_offset: 0,
        })
    }

    pub fn max_level(self) -> u8 {
        self.rule().max_level
    }

    /// `level`에서 다음 단계로 가는 값 (돈, 필요한 길드 레벨).
    pub fn cost(self, level: u8) -> (i64, u32) {
        let r = self.rule();
        (
            r.base_cost * (level as i64 + 1),
            level as u32 + 1 + r.required_level_offset,
        )
    }

    /// 한 단계마다 더하는 것을 사람이 읽는 말로.
    pub fn effect(self, level: u8) -> String {
        let add = self.rule().per_level * level as usize;
        match self {
            Upgrade::Land => format!("땅 +{add}청크"),
            Upgrade::Members => format!("길드원 +{add}명"),
            Upgrade::Storage => format!("창고 +{add}페이지"),
        }
    }
}

/// 2배 이벤트 두 가지.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GuildEvent {
    /// 생활 직업 경험치 2배.
    JobXp,
    /// 높은 등급 재료(광석 덤) 확률 2배.
    RareDrop,
}

impl GuildEvent {
    pub const ALL: [GuildEvent; 2] = [GuildEvent::JobXp, GuildEvent::RareDrop];
    pub fn name(self) -> &'static str {
        match self {
            GuildEvent::JobXp => "생활 직업 경험치 2배",
            GuildEvent::RareDrop => "높은 등급 재료 확률 2배",
        }
    }
    /// 한 번 켜는 값 (GP).
    pub fn cost_gp() -> i64 {
        config::with(|c| c.event_cost_gp)
    }
    /// 켜 두는 시간 (초).
    pub fn duration() -> i64 {
        config::with(|c| c.event_minutes) * 60
    }
}

/// 창고 기록 한 줄.
#[derive(Debug, Clone, PartialEq)]
pub struct LogEntry {
    pub at: i64,
    pub who: String,
    /// 양수 넣음, 음수 뺌.
    pub delta: i32,
    pub item: String,
    pub page: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Request {
    pub uuid: u128,
    pub name: String,
    pub at: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Guild {
    pub id: GuildId,
    pub name: String,
    pub created: i64,
    pub members: Vec<Member>,
    pub description: String,
    /// 길드 땅에 들어올 때·나갈 때 보이는 글.
    pub enter_message: String,
    pub leave_message: String,
    /// 비공개면 목록에 안 나오고 가입 신청을 받지 않습니다.
    pub private: bool,
    /// 길드 경험치 (레벨, 줄지 않음).
    pub exp: i64,
    /// 쓸 수 있는 GP.
    pub gp: i64,
    pub spawn: Option<Spot>,
    pub lands: Vec<Land>,
    /// [`Upgrade::ALL`] 순서의 단계.
    pub upgrades: [u8; 3],
    pub beacon: Vec<beacon::Owned>,
    pub storage: Vec<Vec<Option<Item>>>,
    pub log: Vec<LogEntry>,
    pub requests: Vec<Request>,
    /// 길드원이 아니어도 땅에서 짓고 상자를 열 수 있는 사람.
    pub visitors: Vec<(u128, String)>,
    /// 부길드장·길드원의 권한 (비트).
    pub officer_perms: u32,
    pub member_perms: u32,
    /// [`GuildEvent::ALL`] 순서로, 끝나는 때 (유닉스 초).
    pub events: [i64; 2],
}

impl Guild {
    pub fn new(id: GuildId, name: &str, leader: u128, leader_name: &str, now: i64) -> Self {
        Self {
            id,
            name: name.to_owned(),
            created: now,
            members: vec![Member {
                uuid: leader,
                name: leader_name.to_owned(),
                role: Role::Leader,
                joined: now,
                contribution: 0,
                power: 0,
            }],
            description: String::new(),
            enter_message: format!("{name} 길드의 땅입니다"),
            leave_message: String::new(),
            private: false,
            exp: 0,
            gp: 0,
            spawn: None,
            lands: Vec::new(),
            upgrades: [0; 3],
            beacon: Vec::new(),
            storage: vec![vec![None; PAGE_SLOTS]],
            log: Vec::new(),
            requests: Vec::new(),
            visitors: Vec::new(),
            officer_perms: Perm::defaults(Role::Officer),
            member_perms: Perm::defaults(Role::Member),
            events: [0; 2],
        }
    }

    pub fn leader(&self) -> &Member {
        self.members
            .iter()
            .find(|m| m.role == Role::Leader)
            .unwrap_or(&self.members[0])
    }

    pub fn member(&self, uuid: u128) -> Option<&Member> {
        self.members.iter().find(|m| m.uuid == uuid)
    }

    pub fn member_mut(&mut self, uuid: u128) -> Option<&mut Member> {
        self.members.iter_mut().find(|m| m.uuid == uuid)
    }

    pub fn member_named(&self, name: &str) -> Option<&Member> {
        self.members
            .iter()
            .find(|m| m.name.eq_ignore_ascii_case(name))
    }

    pub fn role_of(&self, uuid: u128) -> Option<Role> {
        self.member(uuid).map(|m| m.role)
    }

    pub fn perms_of(&self, role: Role) -> u32 {
        match role {
            Role::Leader => u32::MAX,
            Role::Officer => self.officer_perms,
            Role::Member => self.member_perms,
        }
    }

    pub fn can(&self, uuid: u128, perm: Perm) -> bool {
        self.role_of(uuid)
            .is_some_and(|r| self.perms_of(r) & perm.bit() != 0)
    }

    /// 레벨 (길드 설정의 레벨 표에서 경험치로).
    pub fn level(&self) -> u32 {
        level_for(self.exp)
    }

    /// 지금 레벨의 효과 줄.
    pub fn level_row(&self) -> config::Level {
        config::with(|c| c.level_row(self.exp).clone())
    }

    pub fn power(&self) -> i64 {
        self.members.iter().map(|m| m.power).sum()
    }

    pub fn upgrade(&self, u: Upgrade) -> u8 {
        self.upgrades[Upgrade::ALL.iter().position(|x| *x == u).unwrap()]
    }

    fn bonus(&self, u: Upgrade) -> usize {
        u.rule().per_level * self.upgrade(u) as usize
    }

    /// 인원 한도 = 레벨 효과 + 업그레이드.
    pub fn max_members(&self) -> usize {
        self.level_row().max_members + self.bonus(Upgrade::Members)
    }
    pub fn max_lands(&self) -> usize {
        self.level_row().max_lands + self.bonus(Upgrade::Land)
    }
    pub fn storage_pages(&self) -> usize {
        (self.level_row().storage_pages + self.bonus(Upgrade::Storage)).clamp(1, 54)
    }

    pub fn event_active(&self, e: GuildEvent, now: i64) -> bool {
        self.events[GuildEvent::ALL.iter().position(|x| *x == e).unwrap()] > now
    }

    /// 창고 페이지 수를 업그레이드에 맞춥니다 (늘기만 함).
    pub fn fit_storage(&mut self) {
        while self.storage.len() < self.storage_pages() {
            self.storage.push(vec![None; PAGE_SLOTS]);
        }
    }

    pub fn push_log(&mut self, e: LogEntry) {
        self.log.push(e);
        if self.log.len() > LOG_KEEP {
            let cut = self.log.len() - LOG_KEEP;
            self.log.drain(..cut);
        }
    }
}

/// 부길드장 최대 수 (설정).
pub fn max_officers() -> usize {
    config::with(|c| c.max_officers)
}

pub fn level_for(exp: i64) -> u32 {
    config::with(|c| c.level_row(exp).level)
}

/// 다음 레벨까지 (지금 레벨 안에서 쌓은 것, 필요한 것). 끝 레벨이면 (0, 0).
pub fn level_progress(exp: i64) -> (i64, i64) {
    config::with(|c| {
        let now = c.level_row(exp);
        match c.next_level(exp) {
            Some(next) => (exp - now.exp, next.exp - now.exp),
            None => (0, 0),
        }
    })
}

/// 길드 이름: 2~12자, 글자·숫자·밑줄만.
pub fn valid_name(name: &str) -> bool {
    let n = name.chars().count();
    (2..=12).contains(&n) && name.chars().all(|c| c.is_alphanumeric() || c == '_')
}

pub fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

/// 길드 일을 해 달라는 쪽 (명령어·창): 누가.
#[derive(Debug, Clone)]
pub struct Actor {
    pub uuid: u128,
    pub name: String,
}

/// 결과: 성공이면 알릴 말, 실패면 이유.
pub type Outcome = Result<String, String>;

impl Data {
    pub fn guild(&self, id: GuildId) -> Option<&Guild> {
        self.guilds.iter().find(|g| g.id == id)
    }

    pub fn guild_mut(&mut self, id: GuildId) -> Option<&mut Guild> {
        self.guilds.iter_mut().find(|g| g.id == id)
    }

    pub fn by_name(&self, name: &str) -> Option<&Guild> {
        self.guilds
            .iter()
            .find(|g| g.name.eq_ignore_ascii_case(name))
    }

    pub fn of(&self, uuid: u128) -> Option<&Guild> {
        self.guilds.iter().find(|g| g.member(uuid).is_some())
    }

    pub(crate) fn of_mut(&mut self, uuid: u128) -> Option<&mut Guild> {
        self.guilds.iter_mut().find(|g| g.member(uuid).is_some())
    }

    /// 내 길드에서 이 권한으로 할 일.
    pub(crate) fn with_perm<R>(
        &mut self,
        actor: &Actor,
        perm: Option<Perm>,
        f: impl FnOnce(&mut Guild) -> Result<R, String>,
    ) -> Result<R, String> {
        let g = self
            .of_mut(actor.uuid)
            .ok_or("길드에 들어가 있지 않습니다.")?;
        match perm {
            Some(p) if !g.can(actor.uuid, p) => Err(format!("권한이 없습니다: {}", p.name())),
            None if g.role_of(actor.uuid) != Some(Role::Leader) => {
                Err("길드장만 할 수 있습니다.".into())
            }
            _ => f(g),
        }
    }

    pub fn create(&mut self, actor: &Actor, name: &str, now: i64) -> Outcome {
        if !valid_name(name) {
            return Err("길드 이름은 2~12자의 글자·숫자·밑줄만 됩니다.".into());
        }
        if self.of(actor.uuid).is_some() {
            return Err("이미 길드에 들어가 있습니다.".into());
        }
        if self.by_name(name).is_some() {
            return Err("같은 이름의 길드가 있습니다.".into());
        }
        self.next_id += 1;
        let g = Guild::new(self.next_id, name, actor.uuid, &actor.name, now);
        self.guilds.push(g);
        // 다른 길드에 낸 가입 신청은 지움
        for g in &mut self.guilds {
            g.requests.retain(|r| r.uuid != actor.uuid);
        }
        Ok(format!("{name} 길드를 만들었습니다."))
    }

    pub fn request_join(&mut self, actor: &Actor, guild: &str, now: i64) -> Outcome {
        if self.of(actor.uuid).is_some() {
            return Err("이미 길드에 들어가 있습니다.".into());
        }
        let g = self
            .guilds
            .iter_mut()
            .find(|g| g.name.eq_ignore_ascii_case(guild))
            .ok_or("그런 길드가 없습니다.")?;
        if g.private {
            return Err("비공개 길드라 가입 신청을 받지 않습니다.".into());
        }
        if g.requests.iter().any(|r| r.uuid == actor.uuid) {
            return Err("이미 신청했습니다.".into());
        }
        g.requests.push(Request {
            uuid: actor.uuid,
            name: actor.name.clone(),
            at: now,
        });
        Ok(format!("{} 길드에 가입을 신청했습니다.", g.name))
    }

    /// 가입 신청 받기(`accept`) 또는 거절.
    pub fn answer_request(&mut self, actor: &Actor, who: u128, accept: bool, now: i64) -> Outcome {
        let already = self.of(who).map(|g| g.id);
        self.with_perm(actor, Some(Perm::AcceptRequests), |g| {
            let i = g
                .requests
                .iter()
                .position(|r| r.uuid == who)
                .ok_or("그 신청이 없습니다.")?;
            let r = g.requests.remove(i);
            if !accept {
                return Ok(format!("{}님의 가입 신청을 거절했습니다.", r.name));
            }
            if already.is_some() {
                return Err(format!("{}님은 이미 다른 길드에 들어갔습니다.", r.name));
            }
            if g.members.len() >= g.max_members() {
                g.requests.insert(i, r);
                return Err("길드 인원이 꽉 찼습니다 (업그레이드로 늘리기).".into());
            }
            g.members.push(Member {
                uuid: r.uuid,
                name: r.name.clone(),
                role: Role::Member,
                joined: now,
                contribution: 0,
                power: 0,
            });
            Ok(format!("{}님이 길드에 들어왔습니다.", r.name))
        })
        .inspect(|_| {
            if accept {
                for g in &mut self.guilds {
                    g.requests.retain(|r| r.uuid != who);
                }
            }
        })
    }

    pub fn leave(&mut self, actor: &Actor) -> Outcome {
        let g = self
            .of_mut(actor.uuid)
            .ok_or("길드에 들어가 있지 않습니다.")?;
        if g.role_of(actor.uuid) == Some(Role::Leader) {
            return Err("길드장은 탈퇴할 수 없습니다. 위임하거나 해산하세요.".into());
        }
        g.members.retain(|m| m.uuid != actor.uuid);
        Ok(format!("{} 길드에서 나왔습니다.", g.name))
    }

    pub fn kick(&mut self, actor: &Actor, target: &str) -> Outcome {
        self.with_perm(actor, Some(Perm::Kick), |g| {
            let t = g
                .member_named(target)
                .ok_or("그런 길드원이 없습니다.")?
                .clone();
            let me = g.role_of(actor.uuid).unwrap();
            if t.uuid == actor.uuid || t.role >= me {
                return Err("자기보다 높거나 같은 역할은 강퇴할 수 없습니다.".into());
            }
            g.members.retain(|m| m.uuid != t.uuid);
            Ok(format!("{}님을 강퇴했습니다.", t.name))
        })
    }

    pub fn transfer_leader(&mut self, actor: &Actor, target: &str) -> Outcome {
        self.with_perm(actor, None, |g| {
            let t = g
                .member_named(target)
                .ok_or("그런 길드원이 없습니다.")?
                .uuid;
            if t == actor.uuid {
                return Err("이미 길드장입니다.".into());
            }
            for m in &mut g.members {
                if m.uuid == actor.uuid {
                    m.role = Role::Officer;
                } else if m.uuid == t {
                    m.role = Role::Leader;
                }
            }
            // 부길드장이 셋이 되면 옛 길드장을 길드원으로
            if g.members.iter().filter(|m| m.role == Role::Officer).count() > max_officers()
                && let Some(m) = g.member_mut(actor.uuid)
            {
                m.role = Role::Member;
            }
            Ok(format!("{target}님에게 길드장을 넘겼습니다."))
        })
    }

    pub fn set_officer(&mut self, actor: &Actor, target: &str, on: bool) -> Outcome {
        self.with_perm(actor, None, |g| {
            let officers = g.members.iter().filter(|m| m.role == Role::Officer).count();
            let m = g
                .members
                .iter_mut()
                .find(|m| m.name.eq_ignore_ascii_case(target))
                .ok_or("그런 길드원이 없습니다.")?;
            match (on, m.role) {
                (_, Role::Leader) => Err("길드장입니다.".into()),
                (true, Role::Officer) => Err("이미 부길드장입니다.".into()),
                (true, _) if officers >= max_officers() => {
                    Err(format!("부길드장은 {}명까지입니다.", max_officers()))
                }
                (true, _) => {
                    m.role = Role::Officer;
                    Ok(format!("{}님을 부길드장으로 임명했습니다.", m.name))
                }
                (false, Role::Officer) => {
                    m.role = Role::Member;
                    Ok(format!("{}님을 길드원으로 돌렸습니다.", m.name))
                }
                (false, _) => Err("부길드장이 아닙니다.".into()),
            }
        })
    }

    pub fn toggle_role_perm(&mut self, actor: &Actor, role: Role, perm: Perm) -> Outcome {
        self.with_perm(actor, None, |g| {
            let bits = match role {
                Role::Officer => &mut g.officer_perms,
                Role::Member => &mut g.member_perms,
                Role::Leader => return Err("길드장은 모든 권한이 있습니다.".into()),
            };
            *bits ^= perm.bit();
            let on = *bits & perm.bit() != 0;
            Ok(format!(
                "{}: {} {}",
                role.name(),
                perm.name(),
                if on { "허용" } else { "막음" }
            ))
        })
    }

    pub fn set_text(&mut self, actor: &Actor, which: Perm, text: &str) -> Outcome {
        let text: String = text.chars().take(120).collect();
        self.with_perm(actor, Some(which), |g| {
            let (slot, what) = match which {
                Perm::Description => (&mut g.description, "길드 설명"),
                Perm::EnterMessage => (&mut g.enter_message, "입장 메시지"),
                Perm::LeaveMessage => (&mut g.leave_message, "퇴장 메시지"),
                _ => return Err("바꿀 수 없는 글입니다.".into()),
            };
            *slot = text;
            Ok(format!("{what}를 바꿨습니다."))
        })
    }

    pub fn toggle_private(&mut self, actor: &Actor) -> Outcome {
        self.with_perm(actor, Some(Perm::Privacy), |g| {
            g.private = !g.private;
            Ok(if g.private {
                "길드를 비공개로 바꿨습니다 (목록에 안 보이고 가입 신청을 받지 않음).".into()
            } else {
                "길드를 공개로 바꿨습니다.".into()
            })
        })
    }

    pub fn set_spawn(&mut self, actor: &Actor, spot: Spot) -> Outcome {
        self.with_perm(actor, Some(Perm::Spawn), |g| {
            g.spawn = Some(spot);
            Ok("길드 스폰을 여기로 정했습니다.".into())
        })
    }

    pub fn visitor(&mut self, actor: &Actor, who: (u128, String), add: bool) -> Outcome {
        self.with_perm(actor, Some(Perm::Visitors), |g| {
            if add {
                if g.member(who.0).is_some() {
                    return Err("길드원은 방문객으로 넣을 필요가 없습니다.".into());
                }
                if g.visitors.iter().any(|v| v.0 == who.0) {
                    return Err("이미 방문객입니다.".into());
                }
                let name = who.1.clone();
                g.visitors.push(who);
                Ok(format!(
                    "{name}님을 방문객으로 넣었습니다 (길드 땅에서 짓기·상자 가능)."
                ))
            } else {
                let before = g.visitors.len();
                g.visitors.retain(|v| v.0 != who.0);
                if g.visitors.len() == before {
                    return Err("방문객이 아닙니다.".into());
                }
                Ok(format!("{}님을 방문객에서 뺐습니다.", who.1))
            }
        })
    }

    pub fn start_event(&mut self, actor: &Actor, e: GuildEvent, now: i64) -> Outcome {
        self.with_perm(actor, Some(Perm::Events), |g| {
            if g.event_active(e, now) {
                return Err(format!("이미 진행 중입니다: {}", e.name()));
            }
            if g.gp < GuildEvent::cost_gp() {
                return Err(format!(
                    "GP가 모자랍니다 ({} / {})",
                    g.gp,
                    GuildEvent::cost_gp()
                ));
            }
            g.gp -= GuildEvent::cost_gp();
            let i = GuildEvent::ALL.iter().position(|x| *x == e).unwrap();
            g.events[i] = now + GuildEvent::duration();
            Ok(format!(
                "{} 이벤트를 {}분 동안 켰습니다.",
                e.name(),
                GuildEvent::duration() / 60
            ))
        })
    }

    /// 업그레이드 한 단계 (돈은 부른 쪽이 먼저 받아 둠). 실패하면 돈을 돌려줘야 합니다.
    pub fn upgrade(&mut self, actor: &Actor, u: Upgrade) -> Outcome {
        self.with_perm(actor, Some(Perm::Upgrade), |g| {
            let i = Upgrade::ALL.iter().position(|x| *x == u).unwrap();
            let level = g.upgrades[i];
            if level >= u.max_level() {
                return Err("이미 끝까지 올렸습니다.".into());
            }
            let (_, need_level) = u.cost(level);
            if g.level() < need_level {
                return Err(format!("길드 레벨 {need_level}이 필요합니다."));
            }
            g.upgrades[i] += 1;
            g.fit_storage();
            Ok(format!(
                "{} {}단계 → {}",
                u.name(),
                level + 1,
                u.effect(level + 1)
            ))
        })
    }

    pub fn disband(&mut self, actor: &Actor) -> Outcome {
        self.with_perm(actor, None, |_| Ok(()))?;
        let i = self
            .guilds
            .iter()
            .position(|g| g.member(actor.uuid).is_some())
            .unwrap();
        let g = self.guilds.remove(i);
        Ok(format!("{} 길드를 해산했습니다.", g.name))
    }

    /// 퀘스트·이벤트 등으로 GP를 받음: 길드 경험치·GP·그 사람 공헌도.
    pub fn add_gp(&mut self, guild: GuildId, uuid: Option<u128>, amount: i64) {
        if let Some(g) = self.guild_mut(guild) {
            g.exp += amount;
            g.gp += amount;
            if let Some(m) = uuid.and_then(|u| g.member_mut(u)) {
                m.contribution += amount;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn actor(n: u128, name: &str) -> Actor {
        Actor {
            uuid: n,
            name: name.into(),
        }
    }

    #[test]
    fn create_join_roles_and_permissions() {
        let mut d = Data::default();
        let (alice, bob, carol, dave) = (
            actor(1, "Alice"),
            actor(2, "Bob"),
            actor(3, "Carol"),
            actor(4, "Dave"),
        );
        assert!(d.create(&alice, "x", 0).is_err());
        d.create(&alice, "달빛", 0).unwrap();
        assert!(d.create(&bob, "달빛", 0).is_err());
        d.request_join(&bob, "달빛", 1).unwrap();
        d.request_join(&carol, "달빛", 1).unwrap();
        d.request_join(&dave, "달빛", 1).unwrap();
        // 길드원은 아직 아님 → 수락 권한 없음
        assert!(d.answer_request(&bob, 3, true, 2).is_err());
        d.answer_request(&alice, 2, true, 2).unwrap();
        d.answer_request(&alice, 3, true, 2).unwrap();
        d.answer_request(&alice, 4, false, 2).unwrap();
        let g = d.of(2).unwrap();
        assert_eq!(g.members.len(), 3);
        assert!(g.requests.is_empty());

        // 부길드장 2명까지, 기본 권한으로 신청 수락은 되고 강퇴는 안 됨
        d.set_officer(&alice, "Bob", true).unwrap();
        d.set_officer(&alice, "Carol", true).unwrap();
        d.request_join(&dave, "달빛", 3).unwrap();
        d.answer_request(&bob, 4, true, 3).unwrap();
        assert!(d.set_officer(&alice, "Dave", true).is_err());
        assert!(d.kick(&bob, "Dave").is_err());
        d.toggle_role_perm(&alice, Role::Officer, Perm::Kick)
            .unwrap();
        // 부길드장끼리는 강퇴 못 함, 길드원은 됨
        assert!(d.kick(&bob, "Carol").is_err());
        d.kick(&bob, "Dave").unwrap();
        assert!(d.toggle_role_perm(&bob, Role::Member, Perm::Kick).is_err());

        // 위임: 부길드장이던 Bob이 길드장이 되며 비운 자리에 옛 길드장이 부길드장으로
        d.transfer_leader(&alice, "Bob").unwrap();
        let g = d.of(1).unwrap();
        assert_eq!(g.role_of(2), Some(Role::Leader));
        assert_eq!(g.role_of(1), Some(Role::Officer));
        assert!(d.leave(&bob).is_err());
        d.leave(&alice).unwrap();
        assert!(d.of(1).is_none());
        assert!(d.disband(&carol).is_err());
        d.disband(&bob).unwrap();
        assert!(d.guilds.is_empty());
    }

    #[test]
    fn private_guilds_refuse_requests() {
        let mut d = Data::default();
        d.create(&actor(1, "A"), "비밀", 0).unwrap();
        d.toggle_private(&actor(1, "A")).unwrap();
        assert!(d.request_join(&actor(2, "B"), "비밀", 0).is_err());
    }

    #[test]
    fn levels_upgrades_and_events() {
        assert_eq!(level_for(0), 1);
        assert_eq!(level_for(999), 1);
        assert_eq!(level_for(1000), 2);
        assert_eq!(level_for(3000), 3);
        assert_eq!(level_progress(1500), (500, 2000));
        let mut d = Data::default();
        let a = actor(1, "A");
        d.create(&a, "길드", 0).unwrap();
        let id = d.of(1).unwrap().id;
        // 레벨 1이면 1단계만 (기본 설정: 레벨 1 창고 1페이지 + 업그레이드 1)
        d.upgrade(&a, Upgrade::Storage).unwrap();
        assert!(d.upgrade(&a, Upgrade::Storage).is_err());
        assert_eq!(d.of(1).unwrap().storage.len(), 2);
        assert_eq!(d.of(1).unwrap().max_members(), 10);
        d.add_gp(id, Some(1), 1000);
        d.upgrade(&a, Upgrade::Storage).unwrap();
        assert_eq!(d.of(1).unwrap().member(1).unwrap().contribution, 1000);
        // 이벤트: GP 500
        d.start_event(&a, GuildEvent::JobXp, 10).unwrap();
        assert!(d.of(1).unwrap().event_active(GuildEvent::JobXp, 11));
        assert!(!d.of(1).unwrap().event_active(GuildEvent::JobXp, 10 + 3600));
        d.start_event(&a, GuildEvent::RareDrop, 10).unwrap();
        assert!(
            d.start_event(&a, GuildEvent::RareDrop, 5000).is_err(),
            "GP 바닥"
        );
    }
}
