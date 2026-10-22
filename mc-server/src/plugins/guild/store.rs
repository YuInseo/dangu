//! 길드 저장: JSON 파일 하나 (`MC_GUILDS`, 기본 `world/guilds.json`).
//!
//! 한 프로세스 안의 인스턴스들은 같은 [`Guilds`]를 씁니다. 담비처럼 프로세스가 여럿이면 같은 파일을
//! 함께 쓰므로, 고칠 때는 잠금 파일(`guilds.json.lock`)을 잡고 → 파일이 바뀌었으면 다시 읽고 →
//! 고치고 → 임시 파일에 쓴 뒤 바꿔치기 합니다. 읽을 때는 0.5초에 한 번만 파일 시각을 봅니다.

use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime};

use super::*;
use crate::json::{self, Json};

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Data {
    pub next_id: GuildId,
    pub guilds: Vec<Guild>,
}

pub struct Guilds {
    path: Option<PathBuf>,
    state: Mutex<State>,
}

struct State {
    data: Data,
    mtime: Option<SystemTime>,
    checked: Option<Instant>,
}

const RECHECK: Duration = Duration::from_millis(500);

static GLOBAL: OnceLock<Guilds> = OnceLock::new();

/// 모든 인스턴스가 함께 쓰는 길드 목록. [`open_global`]을 먼저 부르지 않았으면 메모리에만.
pub fn guilds() -> &'static Guilds {
    GLOBAL.get_or_init(Guilds::in_memory)
}

/// 서버를 켤 때 한 번: 파일에 저장하는 길드 목록으로.
pub fn open_global(path: PathBuf) {
    let _ = GLOBAL.set(Guilds::open(path));
}

impl Guilds {
    pub fn in_memory() -> Self {
        Self {
            path: None,
            state: Mutex::new(State {
                data: Data::default(),
                mtime: None,
                checked: None,
            }),
        }
    }

    pub fn open(path: PathBuf) -> Self {
        let g = Self {
            path: Some(path),
            state: Mutex::new(State {
                data: Data::default(),
                mtime: None,
                checked: None,
            }),
        };
        g.refresh(&mut g.state.lock().unwrap(), true);
        g
    }

    fn mtime(path: &Path) -> Option<SystemTime> {
        std::fs::metadata(path).and_then(|m| m.modified()).ok()
    }

    /// 파일이 바뀌었으면 다시 읽습니다.
    fn refresh(&self, s: &mut State, force: bool) {
        let Some(path) = &self.path else { return };
        if !force && s.checked.is_some_and(|t| t.elapsed() < RECHECK) {
            return;
        }
        s.checked = Some(Instant::now());
        let m = Self::mtime(path);
        if m.is_some() && m != s.mtime {
            if let Some(d) = std::fs::read_to_string(path)
                .ok()
                .and_then(|t| json::parse(&t).ok())
                .and_then(|j| decode(&j))
            {
                s.data = d;
            }
            s.mtime = m;
        }
    }

    pub fn read<R>(&self, f: impl FnOnce(&Data) -> R) -> R {
        let mut s = self.state.lock().unwrap();
        self.refresh(&mut s, false);
        f(&s.data)
    }

    /// 고치고 저장합니다. 다른 프로세스와 겹치지 않게 잠금 파일을 잡습니다.
    pub fn write<R>(&self, f: impl FnOnce(&mut Data) -> R) -> R {
        let mut s = self.state.lock().unwrap();
        let _lock = self.path.as_deref().map(FileLock::take);
        self.refresh(&mut s, true);
        let before = s.data.clone();
        let r = f(&mut s.data);
        if s.data != before
            && let Some(path) = &self.path
        {
            let dir = path.parent().unwrap_or(Path::new("."));
            let name = path.file_name().unwrap().to_string_lossy().into_owned();
            let text = encode(&s.data).pretty();
            if crate::game::playerdata::write_atomically(dir, &name, text.as_bytes()).is_ok() {
                s.mtime = Self::mtime(path);
            }
        }
        r
    }
}

/// `<파일>.lock`을 만들어 잡고, 놓을 때 지웁니다. 30초 넘게 남은 것은 죽은 프로세스의 것으로 봅니다.
struct FileLock(PathBuf);

impl FileLock {
    fn take(path: &Path) -> FileLock {
        let lock = path.with_extension("json.lock");
        if let Some(d) = lock.parent() {
            let _ = std::fs::create_dir_all(d);
        }
        for _ in 0..400 {
            match std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&lock)
            {
                Ok(_) => return FileLock(lock),
                Err(_) => {
                    let stale = std::fs::metadata(&lock)
                        .and_then(|m| m.modified())
                        .ok()
                        .and_then(|t| t.elapsed().ok())
                        .is_some_and(|age| age > Duration::from_secs(30));
                    if stale {
                        let _ = std::fs::remove_file(&lock);
                    } else {
                        std::thread::sleep(Duration::from_millis(5));
                    }
                }
            }
        }
        FileLock(lock)
    }
}

impl Drop for FileLock {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

// ---- JSON ----

fn n(v: f64) -> Json {
    Json::Number(v)
}
fn s(v: &str) -> Json {
    Json::String(v.to_owned())
}
fn obj(f: Vec<(&str, Json)>) -> Json {
    Json::Object(f.into_iter().map(|(k, v)| (k.to_owned(), v)).collect())
}
fn uuid(u: u128) -> Json {
    s(&format!("{u:032x}"))
}

pub fn item_json(i: &Item) -> Json {
    let mut f = vec![
        ("id", s(crate::data::items::name(i.id))),
        ("count", n(i.count as f64)),
    ];
    if let Some(c) = i.custom.and_then(crate::game::custom_items::get) {
        let text = if i.serial != 0 {
            format!("{}#{}", c.id, i.serial)
        } else {
            c.id
        };
        f.push(("custom", s(&text)));
    }
    obj(f)
}

pub fn item_from(j: &Json) -> Option<Item> {
    let id = crate::data::items::by_name(j.get("id")?.as_str()?)?;
    let count = j.get("count")?.as_f64()? as i32;
    let (custom, serial) = match j.get("custom").and_then(Json::as_str) {
        Some(t) => {
            let (name, serial) = t.split_once('#').unwrap_or((t, "0"));
            (
                Some(crate::game::custom_items::index_of(name)?),
                serial.parse().unwrap_or(0),
            )
        }
        None => (None, 0),
    };
    Some(Item {
        id,
        count,
        custom,
        serial,
    })
}

fn spot_json(p: &Spot) -> Json {
    obj(vec![
        ("instance", s(&p.instance)),
        ("dimension", s(p.dimension.key())),
        ("x", n(p.pos.0)),
        ("y", n(p.pos.1)),
        ("z", n(p.pos.2)),
    ])
}

fn spot_from(j: &Json) -> Option<Spot> {
    Some(Spot {
        instance: j.get("instance")?.as_str()?.to_owned(),
        dimension: Dimension::from_key(j.get("dimension")?.as_str()?)?,
        pos: (
            j.get("x")?.as_f64()?,
            j.get("y")?.as_f64()?,
            j.get("z")?.as_f64()?,
        ),
    })
}

pub fn encode(d: &Data) -> Json {
    let guilds = d
        .guilds
        .iter()
        .map(|g| {
            let mut f = vec![
                ("id", n(g.id as f64)),
                ("name", s(&g.name)),
                ("created", n(g.created as f64)),
                (
                    "members",
                    Json::Array(
                        g.members
                            .iter()
                            .map(|m| {
                                obj(vec![
                                    ("uuid", uuid(m.uuid)),
                                    ("name", s(&m.name)),
                                    ("role", s(m.role.key())),
                                    ("joined", n(m.joined as f64)),
                                    ("contribution", n(m.contribution as f64)),
                                    ("power", n(m.power as f64)),
                                ])
                            })
                            .collect(),
                    ),
                ),
                ("description", s(&g.description)),
                ("enter_message", s(&g.enter_message)),
                ("leave_message", s(&g.leave_message)),
                ("private", Json::Bool(g.private)),
                ("exp", n(g.exp as f64)),
                ("gp", n(g.gp as f64)),
                (
                    "lands",
                    Json::Array(
                        g.lands
                            .iter()
                            .map(|l| {
                                obj(vec![
                                    ("instance", s(&l.instance)),
                                    ("dimension", s(l.dimension.key())),
                                    ("cx", n(l.chunk.0 as f64)),
                                    ("cz", n(l.chunk.1 as f64)),
                                    ("bought", n(l.bought as f64)),
                                    ("paid", n(l.paid as f64)),
                                ])
                            })
                            .collect(),
                    ),
                ),
                (
                    "upgrades",
                    Json::Array(g.upgrades.iter().map(|&u| n(u as f64)).collect()),
                ),
                (
                    "beacon",
                    Json::Array(
                        g.beacon
                            .iter()
                            .map(|b| {
                                obj(vec![
                                    ("effect", s(b.effect.key())),
                                    ("tier", n(b.tier as f64)),
                                ])
                            })
                            .collect(),
                    ),
                ),
                (
                    "storage",
                    Json::Array(
                        g.storage
                            .iter()
                            .map(|page| {
                                Json::Array(
                                    page.iter()
                                        .map(|slot| slot.as_ref().map_or(Json::Null, item_json))
                                        .collect(),
                                )
                            })
                            .collect(),
                    ),
                ),
                (
                    "log",
                    Json::Array(
                        g.log
                            .iter()
                            .map(|e| {
                                obj(vec![
                                    ("at", n(e.at as f64)),
                                    ("who", s(&e.who)),
                                    ("delta", n(e.delta as f64)),
                                    ("item", s(&e.item)),
                                    ("page", n(e.page as f64)),
                                ])
                            })
                            .collect(),
                    ),
                ),
                (
                    "requests",
                    Json::Array(
                        g.requests
                            .iter()
                            .map(|r| {
                                obj(vec![
                                    ("uuid", uuid(r.uuid)),
                                    ("name", s(&r.name)),
                                    ("at", n(r.at as f64)),
                                ])
                            })
                            .collect(),
                    ),
                ),
                (
                    "visitors",
                    Json::Array(
                        g.visitors
                            .iter()
                            .map(|(u, name)| obj(vec![("uuid", uuid(*u)), ("name", s(name))]))
                            .collect(),
                    ),
                ),
                ("officer_perms", n(g.officer_perms as f64)),
                ("member_perms", n(g.member_perms as f64)),
                (
                    "events",
                    Json::Array(g.events.iter().map(|&e| n(e as f64)).collect()),
                ),
                ("quests", g.quests.to_json()),
            ];
            if let Some(sp) = &g.spawn {
                f.push(("spawn", spot_json(sp)));
            }
            obj(f)
        })
        .collect();
    obj(vec![
        ("next_id", n(d.next_id as f64)),
        ("guilds", Json::Array(guilds)),
    ])
}

fn arr(j: &Json, k: &str) -> Vec<Json> {
    j.get(k)
        .and_then(Json::as_array)
        .map(<[Json]>::to_vec)
        .unwrap_or_default()
}
fn num(j: &Json, k: &str) -> f64 {
    j.get(k).and_then(Json::as_f64).unwrap_or(0.0)
}
fn text(j: &Json, k: &str) -> String {
    j.get(k).and_then(Json::as_str).unwrap_or("").to_owned()
}
fn uuid_of(j: &Json, k: &str) -> Option<u128> {
    u128::from_str_radix(j.get(k)?.as_str()?, 16).ok()
}

pub fn decode(j: &Json) -> Option<Data> {
    let mut d = Data {
        next_id: num(j, "next_id") as GuildId,
        guilds: Vec::new(),
    };
    for g in arr(j, "guilds") {
        let members = arr(&g, "members")
            .iter()
            .filter_map(|m| {
                Some(Member {
                    uuid: uuid_of(m, "uuid")?,
                    name: text(m, "name"),
                    role: Role::parse(&text(m, "role"))?,
                    joined: num(m, "joined") as i64,
                    contribution: num(m, "contribution") as i64,
                    power: num(m, "power") as i64,
                })
            })
            .collect::<Vec<_>>();
        if members.is_empty() {
            continue;
        }
        let mut upgrades = [0u8; 4];
        for (i, u) in arr(&g, "upgrades").iter().take(4).enumerate() {
            upgrades[i] = u.as_f64().unwrap_or(0.0) as u8;
        }
        let mut events = [0i64; 2];
        for (i, e) in arr(&g, "events").iter().take(2).enumerate() {
            events[i] = e.as_f64().unwrap_or(0.0) as i64;
        }
        let mut guild = Guild {
            id: num(&g, "id") as GuildId,
            name: text(&g, "name"),
            created: num(&g, "created") as i64,
            members,
            description: text(&g, "description"),
            enter_message: text(&g, "enter_message"),
            leave_message: text(&g, "leave_message"),
            private: g.get("private").and_then(Json::as_bool).unwrap_or(false),
            exp: num(&g, "exp") as i64,
            gp: num(&g, "gp") as i64,
            spawn: g.get("spawn").and_then(spot_from),
            lands: arr(&g, "lands")
                .iter()
                .filter_map(|l| {
                    Some(Land {
                        instance: text(l, "instance"),
                        dimension: Dimension::from_key(&text(l, "dimension"))?,
                        chunk: (num(l, "cx") as i32, num(l, "cz") as i32),
                        bought: num(l, "bought") as i64,
                        paid: num(l, "paid") as i64,
                    })
                })
                .collect(),
            upgrades,
            beacon: arr(&g, "beacon")
                .iter()
                .filter_map(|b| {
                    Some(beacon::Owned {
                        effect: beacon::Effect::from_key(&text(b, "effect"))?,
                        tier: num(b, "tier") as u8,
                    })
                })
                .collect(),
            storage: arr(&g, "storage")
                .iter()
                .map(|page| {
                    let mut slots: Vec<Option<Item>> = page
                        .as_array()
                        .unwrap_or_default()
                        .iter()
                        .map(item_from)
                        .collect();
                    slots.resize(PAGE_SLOTS, None);
                    slots
                })
                .collect(),
            log: arr(&g, "log")
                .iter()
                .map(|e| LogEntry {
                    at: num(e, "at") as i64,
                    who: text(e, "who"),
                    delta: num(e, "delta") as i32,
                    item: text(e, "item"),
                    page: num(e, "page") as usize,
                })
                .collect(),
            requests: arr(&g, "requests")
                .iter()
                .filter_map(|r| {
                    Some(Request {
                        uuid: uuid_of(r, "uuid")?,
                        name: text(r, "name"),
                        at: num(r, "at") as i64,
                    })
                })
                .collect(),
            visitors: arr(&g, "visitors")
                .iter()
                .filter_map(|v| Some((uuid_of(v, "uuid")?, text(v, "name"))))
                .collect(),
            officer_perms: num(&g, "officer_perms") as u32,
            member_perms: num(&g, "member_perms") as u32,
            events,
            quests: g
                .get("quests")
                .map(quest::Board::from_json)
                .unwrap_or_default(),
        };
        guild.fit_storage();
        d.guilds.push(guild);
    }
    Some(d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_through_file_and_another_process_view() {
        let dir = std::env::temp_dir().join(format!("dangu-guilds-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("guilds.json");
        let a = Guilds::open(path.clone());
        let b = Guilds::open(path.clone());
        let alice = Actor {
            uuid: 7,
            name: "Alice".into(),
        };
        a.write(|d| {
            d.create(&alice, "달빛", 100).unwrap();
            let g = d.guilds.last_mut().unwrap();
            g.storage[0][3] = crate::data::items::by_name("diamond").map(|id| Item::new(id, 5));
            g.lands.push(Land {
                instance: "main".into(),
                dimension: Dimension::Overworld,
                chunk: (3, -2),
                bought: 1,
                paid: 10_000,
            });
            g.spawn = Some(Spot {
                instance: "main".into(),
                dimension: Dimension::Nether,
                pos: (1.5, 64.0, -3.5),
            });
        });
        // 다른 프로세스(= 다른 Guilds)가 고치면 a도 다시 읽음
        b.write(|d| d.add_gp(1, Some(7), 1500));
        std::thread::sleep(RECHECK + Duration::from_millis(50));
        let back = a.read(|d| d.clone());
        let g = &back.guilds[0];
        assert_eq!(g.name, "달빛");
        assert_eq!(g.exp, 1500);
        assert_eq!(g.storage[0][3].map(|i| i.count), Some(5));
        assert_eq!(g.lands[0].chunk, (3, -2));
        assert_eq!(g.spawn.as_ref().unwrap().dimension, Dimension::Nether);
        assert_eq!(decode(&encode(&back)).unwrap(), back);
        assert!(!path.with_extension("json.lock").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
