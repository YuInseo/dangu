//! 월드 폴더의 신분증 `level.json`: 이름, 시드, 만든 때, 지형 생성기 판, 스폰 지점.
//!
//! 서버를 처음 켤 때(또는 새 인스턴스를 만들 때) 폴더에 이 파일이 없으면 새 월드로 보고
//! 만듭니다. 시드가 이미 적혀 있으면 그 시드를 따릅니다 — 설정의 시드가 바뀌어도 기존 월드의
//! 지형이 갑자기 바뀌지 않도록.

use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use super::{Dimension, terrain};
use crate::json::{self, Json};

pub const FILE_NAME: &str = "level.json";
/// 지형 생성 규칙의 판. 생성기가 바뀌면 올려서, 예전 판으로 만든 월드를 알아볼 수 있게.
pub const GENERATOR_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq)]
pub struct Level {
    pub name: String,
    pub seed: u64,
    pub created: u64,
    pub generator: u32,
    pub spawn: (i32, i32, i32),
}

impl Level {
    fn to_json(&self) -> Json {
        let n = |v: f64| Json::Number(v);
        Json::Object(vec![
            ("name".into(), Json::String(self.name.clone())),
            ("seed".into(), Json::String(self.seed.to_string())),
            ("created".into(), n(self.created as f64)),
            ("generator".into(), n(self.generator as f64)),
            (
                "spawn".into(),
                Json::Array(vec![
                    n(self.spawn.0 as f64),
                    n(self.spawn.1 as f64),
                    n(self.spawn.2 as f64),
                ]),
            ),
        ])
    }

    fn from_json(v: &Json) -> Option<Self> {
        let num = |k: &str| match v.get(k)? {
            Json::Number(n) => Some(*n),
            _ => None,
        };
        let spawn = v.get("spawn")?.as_array()?;
        let c = |i: usize| match spawn.get(i)? {
            Json::Number(n) => Some(*n as i32),
            _ => None,
        };
        Some(Self {
            name: v.get("name")?.as_str()?.to_owned(),
            seed: v.get("seed")?.as_str()?.parse().ok()?,
            created: num("created")? as u64,
            generator: num("generator")? as u32,
            spawn: (c(0)?, c(1)?, c(2)?),
        })
    }
}

/// 바닐라가 알아보는 `level.dat` (gzip NBT). 바닐라 서버·도구가 이 폴더를 월드로 열 수 있게.
pub fn write_level_dat(dir: &Path, level: &Level, state: &LevelState) -> std::io::Result<()> {
    use crate::protocol::nbt::Tag;
    let s = |v: &str| Tag::String(v.to_owned());
    let dim = |ty: &str, settings: &str, source: Tag| {
        Tag::Compound(vec![
            ("type".into(), s(ty)),
            (
                "generator".into(),
                Tag::Compound(vec![
                    ("type".into(), s("minecraft:noise")),
                    ("settings".into(), s(settings)),
                    ("biome_source".into(), source),
                ]),
            ),
        ])
    };
    let preset = |p: &str| {
        Tag::Compound(vec![
            ("type".into(), s("minecraft:multi_noise")),
            ("preset".into(), s(p)),
        ])
    };
    let data = Tag::Compound(vec![
        ("DataVersion".into(), Tag::Int(super::anvil::DATA_VERSION)),
        (
            "version".into(),
            Tag::Compound(vec![
                ("Id".into(), Tag::Int(super::anvil::DATA_VERSION)),
                ("Name".into(), s(crate::protocol::VERSION_NAME)),
                ("Series".into(), s("main")),
                ("Snapshot".into(), Tag::Byte(0)),
            ]),
        ),
        ("LevelName".into(), s(&level.name)),
        (
            "WorldGenSettings".into(),
            Tag::Compound(vec![
                ("seed".into(), Tag::Long(level.seed as i64)),
                ("generate_features".into(), Tag::Byte(1)),
                ("bonus_chest".into(), Tag::Byte(0)),
                (
                    "dimensions".into(),
                    Tag::Compound(vec![
                        (
                            "minecraft:overworld".into(),
                            dim(
                                "minecraft:overworld",
                                "minecraft:overworld",
                                preset("minecraft:overworld"),
                            ),
                        ),
                        (
                            "minecraft:the_nether".into(),
                            dim(
                                "minecraft:the_nether",
                                "minecraft:nether",
                                preset("minecraft:nether"),
                            ),
                        ),
                        (
                            "minecraft:the_end".into(),
                            dim(
                                "minecraft:the_end",
                                "minecraft:end",
                                Tag::Compound(vec![("type".into(), s("minecraft:the_end"))]),
                            ),
                        ),
                    ]),
                ),
            ]),
        ),
        ("SpawnX".into(), Tag::Int(level.spawn.0)),
        ("SpawnY".into(), Tag::Int(level.spawn.1)),
        ("SpawnZ".into(), Tag::Int(level.spawn.2)),
        (
            "spawn".into(),
            Tag::Compound(vec![
                ("dimension".into(), s("minecraft:overworld")),
                (
                    "pos".into(),
                    Tag::IntArray(vec![level.spawn.0, level.spawn.1, level.spawn.2]),
                ),
                ("yaw".into(), Tag::Float(0.0)),
                ("pitch".into(), Tag::Float(0.0)),
            ]),
        ),
        ("GameType".into(), Tag::Int(state.gamemode)),
        ("Difficulty".into(), Tag::Byte(state.difficulty as i8)),
        ("hardcore".into(), Tag::Byte(state.hardcore as i8)),
        ("allowCommands".into(), Tag::Byte(1)),
        ("initialized".into(), Tag::Byte(1)),
        ("Time".into(), Tag::Long(state.time)),
        ("DayTime".into(), Tag::Long(state.day_time)),
        (
            "LastPlayed".into(),
            Tag::Long(
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map_or(0, |d| d.as_millis() as i64),
            ),
        ),
        ("raining".into(), Tag::Byte(0)),
        ("thundering".into(), Tag::Byte(0)),
        ("ServerBrands".into(), Tag::List(vec![s("hoya")])),
    ]);
    let mut raw = Vec::new();
    Tag::Compound(vec![("Data".into(), data)]).write_named("", &mut raw);
    crate::game::playerdata::write_atomically(dir, "level.dat", &crate::protocol::zlib::gzip(&raw))
}

/// `level.dat`에 쓰는 바뀌는 값들.
#[derive(Debug, Clone, Copy, Default)]
pub struct LevelState {
    pub gamemode: i32,
    pub difficulty: u8,
    pub hardcore: bool,
    pub time: i64,
    pub day_time: i64,
}

pub fn read(dir: &Path) -> Option<Level> {
    let text = std::fs::read_to_string(dir.join(FILE_NAME)).ok()?;
    Level::from_json(&json::parse(&text).ok()?)
}

/// 월드 폴더를 준비합니다. 새 월드면 `level.json`을 만들고 `true`.
/// 이미 있으면 적힌 시드가 `seed`와 다를 때 경고만 하고 그대로 둡니다.
pub fn ensure(dir: &Path, name: &str, seed: u64) -> std::io::Result<bool> {
    if let Some(level) = read(dir) {
        if level.seed != seed {
            eprintln!(
                "{}: 설정의 시드 {seed}와 월드의 시드 {}가 다릅니다 — 월드의 시드는 instances.json에서 바꿔 주세요.",
                dir.display(),
                level.seed
            );
        }
        return Ok(false);
    }
    std::fs::create_dir_all(dir)?;
    let spawn = terrain::spawn_point(Dimension::Overworld, seed);
    let level = Level {
        name: name.to_owned(),
        seed,
        created: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_secs()),
        generator: GENERATOR_VERSION,
        spawn: (
            spawn.0.floor() as i32,
            spawn.1.floor() as i32,
            spawn.2.floor() as i32,
        ),
    };
    crate::game::playerdata::write_atomically(dir, FILE_NAME, level.to_json().pretty().as_bytes())?;
    write_level_dat(
        dir,
        &level,
        &LevelState {
            difficulty: 2,
            ..LevelState::default()
        },
    )?;
    println!(
        "새 월드 {name} 만듦 (시드 {seed}, 스폰 {} {} {})",
        level.spawn.0, level.spawn.1, level.spawn.2
    );
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_once_and_keeps_seed() {
        let dir = std::env::temp_dir().join(format!("hoya-level-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        assert!(ensure(&dir, "t", 42).unwrap());
        // level.dat은 gzip NBT이고 시드가 들어 있음
        let raw =
            crate::protocol::zlib::gunzip(&std::fs::read(dir.join("level.dat")).unwrap(), 1 << 20)
                .unwrap();
        let (_, root) = crate::protocol::nbt::Tag::read_named(&raw).unwrap();
        let seed = root
            .get("Data")
            .and_then(|d| d.get("WorldGenSettings"))
            .and_then(|w| w.get("seed"));
        assert_eq!(seed.and_then(crate::protocol::nbt::Tag::as_i64), Some(42));
        assert!(!ensure(&dir, "t", 43).unwrap());
        let level = read(&dir).unwrap();
        assert_eq!(
            (level.name.as_str(), level.seed, level.generator),
            ("t", 42, GENERATOR_VERSION)
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
