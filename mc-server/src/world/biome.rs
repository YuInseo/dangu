//! 바이옴: 기후(온도·습도·대륙성·침식·기묘함) 노이즈로 고르고, 바이옴마다 겉흙·나무·풀·
//! 구조물이 달라집니다. 바닐라의 다중 노이즈 바이옴 고르기를 그대로 옮기지는 않고, 같은
//! 기후 축을 써서 비슷한 배치(추운 곳은 눈, 마른 곳은 사막, 산은 낮은 침식)를 만듭니다.
//!
//! 지하에는 3차원 바이옴(동굴 바이옴)이 따로 있습니다: 무성한 동굴, 점적석 동굴, 깊은 어둠,
//! 그리고 26.3의 유황 동굴.

use std::collections::HashMap;
use std::sync::OnceLock;

use super::noise::{fbm, hash};
use crate::data::registry_id;

/// 바이옴 하나 = [`BIOMES`]의 번호.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Biome(pub u8);

/// 나무 종류. 모양은 [`super::features`]에서 만듭니다.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tree {
    Oak,
    FancyOak,
    Birch,
    TallBirch,
    Spruce,
    Pine,
    MegaSpruce,
    Jungle,
    MegaJungle,
    Acacia,
    DarkOak,
    PaleOak,
    Cherry,
    Mangrove,
    Poplar,
    HugeMushroom,
}

/// 땅 위에 흩뿌리는 것들.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Plant {
    Grass,
    TallGrass,
    Fern,
    Flowers,
    Sunflowers,
    Cactus,
    DeadBush,
    DryGrass,
    SugarCane,
    Pumpkin,
    Melon,
    SweetBerries,
    Mushrooms,
    Bamboo,
    LilyPad,
    LeafLitter,
    FireflyBush,
    Bush,
    PaleMoss,
    Wildflowers,
    Seagrass,
    Kelp,
}

pub struct BiomeDef {
    pub name: &'static str,
    /// 맨 위 블록, 그 아래 3~4칸, 물 밑 바닥.
    pub top: &'static str,
    pub under: &'static str,
    pub floor: &'static str,
    /// 눈이 쌓이고 물이 얼음.
    pub snowy: bool,
    /// (나무, 가중치). 비어 있으면 나무 없음.
    pub trees: &'static [(Tree, u32)],
    /// 청크마다 나무 수 (10배; 5 = 청크 둘에 하나).
    pub tree_density: u32,
    /// (풀, 청크마다 시도 수).
    pub plants: &'static [(Plant, u32)],
    /// 이 바이옴 꽃들.
    pub flowers: &'static [&'static str],
}

macro_rules! biome {
    ($name:literal, $top:literal, $under:literal, $floor:literal, snowy: $snowy:expr,
     trees: [$($t:ident $tw:literal),*], density: $d:literal,
     plants: [$($p:ident $pn:literal),*], flowers: [$($f:literal),*]) => {
        BiomeDef {
            name: $name,
            top: $top,
            under: $under,
            floor: $floor,
            snowy: $snowy,
            trees: &[$((Tree::$t, $tw)),*],
            tree_density: $d,
            plants: &[$((Plant::$p, $pn)),*],
            flowers: &[$($f),*],
        }
    };
}

/// 쓰는 바이옴들. 순서가 [`Biome`] 번호입니다.
pub const BIOMES: &[BiomeDef] = &[
    // ---- 오버월드 지상 ----
    biome!("plains", "grass_block", "dirt", "sand", snowy: false,
        trees: [Oak 9, FancyOak 1], density: 1,
        plants: [Grass 20, TallGrass 3, Flowers 3, Bush 1, SugarCane 2, Pumpkin 1, FireflyBush 1],
        flowers: ["dandelion", "poppy", "azure_bluet", "oxeye_daisy", "cornflower", "red_tulip", "orange_tulip"]),
    biome!("sunflower_plains", "grass_block", "dirt", "sand", snowy: false,
        trees: [Oak 1], density: 1,
        plants: [Grass 20, Sunflowers 6, Flowers 3],
        flowers: ["dandelion", "poppy", "oxeye_daisy"]),
    biome!("forest", "grass_block", "dirt", "sand", snowy: false,
        trees: [Oak 8, Birch 2, FancyOak 1], density: 80,
        plants: [Grass 6, Flowers 2, Mushrooms 1, LeafLitter 2, Bush 1],
        flowers: ["dandelion", "poppy", "lily_of_the_valley"]),
    biome!("flower_forest", "grass_block", "dirt", "sand", snowy: false,
        trees: [Oak 7, Birch 3, FancyOak 1], density: 40,
        plants: [Grass 4, Flowers 30, Wildflowers 4],
        flowers: ["allium", "azure_bluet", "red_tulip", "orange_tulip", "white_tulip", "pink_tulip", "oxeye_daisy", "cornflower", "lily_of_the_valley", "dandelion", "poppy"]),
    biome!("birch_forest", "grass_block", "dirt", "sand", snowy: false,
        trees: [Birch 1], density: 70,
        plants: [Grass 6, Flowers 2, Wildflowers 3, LeafLitter 2],
        flowers: ["dandelion", "poppy", "lily_of_the_valley"]),
    biome!("old_growth_birch_forest", "grass_block", "dirt", "sand", snowy: false,
        trees: [TallBirch 1], density: 80,
        plants: [Grass 6, Flowers 2],
        flowers: ["dandelion", "poppy"]),
    biome!("dark_forest", "grass_block", "dirt", "sand", snowy: false,
        trees: [DarkOak 6, Oak 1, Birch 1, HugeMushroom 1], density: 160,
        plants: [Grass 4, Mushrooms 2, LeafLitter 3],
        flowers: ["poppy", "dandelion"]),
    biome!("pale_garden", "grass_block", "dirt", "sand", snowy: false,
        trees: [PaleOak 1], density: 120,
        plants: [PaleMoss 8, Grass 2],
        flowers: ["closed_eyeblossom", "open_eyeblossom"]),
    // 26.3: 붉고 노란 포플러가 섞인 숲
    biome!("dappled_forest", "grass_block", "dirt", "sand", snowy: false,
        trees: [Poplar 5, Birch 2, Oak 2], density: 70,
        plants: [Grass 6, LeafLitter 6, Mushrooms 1, Bush 1],
        flowers: ["dandelion", "poppy"]),
    biome!("cherry_grove", "grass_block", "dirt", "sand", snowy: false,
        trees: [Cherry 1], density: 25,
        plants: [Grass 8, Flowers 5],
        flowers: ["pink_petals"]),
    biome!("meadow", "grass_block", "dirt", "sand", snowy: false,
        trees: [Oak 1, Birch 1], density: 1,
        plants: [Grass 25, TallGrass 5, Flowers 10, Wildflowers 3],
        flowers: ["allium", "azure_bluet", "cornflower", "oxeye_daisy", "dandelion", "poppy"]),
    biome!("taiga", "grass_block", "dirt", "sand", snowy: false,
        trees: [Spruce 2, Pine 1], density: 70,
        plants: [Grass 4, Fern 6, SweetBerries 1, Mushrooms 1],
        flowers: ["dandelion"]),
    biome!("old_growth_spruce_taiga", "podzol", "dirt", "sand", snowy: false,
        trees: [MegaSpruce 3, Spruce 2], density: 100,
        plants: [Fern 8, SweetBerries 1, Mushrooms 2],
        flowers: []),
    biome!("old_growth_pine_taiga", "podzol", "dirt", "sand", snowy: false,
        trees: [MegaSpruce 2, Pine 2, Spruce 1], density: 100,
        plants: [Fern 8, SweetBerries 1, Mushrooms 2],
        flowers: []),
    biome!("snowy_taiga", "grass_block", "dirt", "sand", snowy: true,
        trees: [Spruce 2, Pine 1], density: 50,
        plants: [Fern 3, SweetBerries 1],
        flowers: []),
    biome!("snowy_plains", "grass_block", "dirt", "sand", snowy: true,
        trees: [Spruce 1], density: 1,
        plants: [Grass 2],
        flowers: []),
    biome!("ice_spikes", "snow_block", "dirt", "sand", snowy: true,
        trees: [], density: 0, plants: [], flowers: []),
    biome!("desert", "sand", "sandstone", "sand", snowy: false,
        trees: [], density: 0,
        plants: [Cactus 3, DeadBush 3, DryGrass 4],
        flowers: []),
    biome!("savanna", "grass_block", "dirt", "sand", snowy: false,
        trees: [Acacia 4, Oak 1], density: 10,
        plants: [Grass 20, TallGrass 8, DryGrass 2],
        flowers: ["dandelion", "poppy"]),
    biome!("savanna_plateau", "grass_block", "dirt", "sand", snowy: false,
        trees: [Acacia 4, Oak 1], density: 10,
        plants: [Grass 20, TallGrass 8],
        flowers: ["dandelion"]),
    biome!("jungle", "grass_block", "dirt", "sand", snowy: false,
        trees: [Jungle 4, MegaJungle 1, FancyOak 1], density: 120,
        plants: [Grass 20, Fern 5, Melon 1, Bamboo 1],
        flowers: ["dandelion", "poppy"]),
    biome!("sparse_jungle", "grass_block", "dirt", "sand", snowy: false,
        trees: [Jungle 3, Oak 1], density: 20,
        plants: [Grass 20, Fern 4, Melon 1],
        flowers: ["dandelion", "poppy"]),
    biome!("bamboo_jungle", "podzol", "dirt", "sand", snowy: false,
        trees: [Jungle 2, MegaJungle 1], density: 40,
        plants: [Bamboo 12, Grass 10, Melon 1],
        flowers: ["dandelion"]),
    biome!("swamp", "grass_block", "dirt", "mud", snowy: false,
        trees: [Oak 1], density: 20,
        plants: [Grass 5, LilyPad 4, Mushrooms 2, SugarCane 3, FireflyBush 2],
        flowers: ["blue_orchid"]),
    biome!("mangrove_swamp", "mud", "mud", "mud", snowy: false,
        trees: [Mangrove 1], density: 60,
        plants: [Grass 2, LilyPad 3],
        flowers: []),
    biome!("badlands", "red_sand", "terracotta", "red_sand", snowy: false,
        trees: [], density: 0, plants: [DeadBush 2, Cactus 1, DryGrass 2], flowers: []),
    biome!("eroded_badlands", "red_sand", "terracotta", "red_sand", snowy: false,
        trees: [], density: 0, plants: [DeadBush 2, DryGrass 1], flowers: []),
    biome!("wooded_badlands", "coarse_dirt", "terracotta", "red_sand", snowy: false,
        trees: [Oak 1], density: 15, plants: [DeadBush 2, Grass 3], flowers: []),
    biome!("mushroom_fields", "mycelium", "dirt", "gravel", snowy: false,
        trees: [HugeMushroom 1], density: 6, plants: [Mushrooms 4], flowers: []),
    biome!("windswept_hills", "grass_block", "dirt", "gravel", snowy: false,
        trees: [Spruce 1, Oak 1], density: 3, plants: [Grass 6], flowers: []),
    biome!("windswept_forest", "grass_block", "dirt", "gravel", snowy: false,
        trees: [Spruce 2, Oak 1], density: 50, plants: [Grass 5], flowers: []),
    biome!("windswept_gravelly_hills", "gravel", "gravel", "gravel", snowy: false,
        trees: [Spruce 1], density: 1, plants: [], flowers: []),
    biome!("windswept_savanna", "grass_block", "dirt", "gravel", snowy: false,
        trees: [Acacia 1], density: 4, plants: [Grass 10], flowers: []),
    biome!("grove", "snow_block", "dirt", "gravel", snowy: true,
        trees: [Spruce 1], density: 50, plants: [], flowers: []),
    biome!("snowy_slopes", "snow_block", "snow_block", "gravel", snowy: true,
        trees: [], density: 0, plants: [], flowers: []),
    biome!("frozen_peaks", "packed_ice", "snow_block", "gravel", snowy: true,
        trees: [], density: 0, plants: [], flowers: []),
    biome!("jagged_peaks", "snow_block", "snow_block", "gravel", snowy: true,
        trees: [], density: 0, plants: [], flowers: []),
    biome!("stony_peaks", "stone", "stone", "gravel", snowy: false,
        trees: [], density: 0, plants: [], flowers: []),
    biome!("beach", "sand", "sand", "sand", snowy: false,
        trees: [], density: 0, plants: [], flowers: []),
    biome!("snowy_beach", "sand", "sand", "sand", snowy: true,
        trees: [], density: 0, plants: [], flowers: []),
    biome!("stony_shore", "stone", "stone", "gravel", snowy: false,
        trees: [], density: 0, plants: [], flowers: []),
    biome!("river", "sand", "dirt", "sand", snowy: false,
        trees: [], density: 0, plants: [Seagrass 6, SugarCane 2], flowers: []),
    biome!("frozen_river", "sand", "dirt", "sand", snowy: true,
        trees: [], density: 0, plants: [], flowers: []),
    // ---- 바다 ----
    biome!("ocean", "gravel", "gravel", "gravel", snowy: false,
        trees: [], density: 0, plants: [Seagrass 10, Kelp 4], flowers: []),
    biome!("deep_ocean", "gravel", "gravel", "gravel", snowy: false,
        trees: [], density: 0, plants: [Seagrass 6, Kelp 6], flowers: []),
    biome!("cold_ocean", "gravel", "gravel", "gravel", snowy: false,
        trees: [], density: 0, plants: [Seagrass 6, Kelp 3], flowers: []),
    biome!("deep_cold_ocean", "gravel", "gravel", "gravel", snowy: false,
        trees: [], density: 0, plants: [Seagrass 4, Kelp 4], flowers: []),
    biome!("frozen_ocean", "gravel", "gravel", "gravel", snowy: true,
        trees: [], density: 0, plants: [], flowers: []),
    biome!("deep_frozen_ocean", "gravel", "gravel", "gravel", snowy: true,
        trees: [], density: 0, plants: [], flowers: []),
    biome!("lukewarm_ocean", "sand", "sand", "sand", snowy: false,
        trees: [], density: 0, plants: [Seagrass 12, Kelp 3], flowers: []),
    biome!("deep_lukewarm_ocean", "sand", "sand", "sand", snowy: false,
        trees: [], density: 0, plants: [Seagrass 8, Kelp 4], flowers: []),
    biome!("warm_ocean", "sand", "sand", "sand", snowy: false,
        trees: [], density: 0, plants: [Seagrass 14], flowers: []),
    // ---- 동굴 (3차원) ----
    biome!("lush_caves", "moss_block", "clay", "clay", snowy: false,
        trees: [], density: 0, plants: [], flowers: []),
    biome!("dripstone_caves", "dripstone_block", "stone", "stone", snowy: false,
        trees: [], density: 0, plants: [], flowers: []),
    biome!("deep_dark", "sculk", "deepslate", "deepslate", snowy: false,
        trees: [], density: 0, plants: [], flowers: []),
    biome!("sulfur_caves", "sulfur", "stone", "stone", snowy: false,
        trees: [], density: 0, plants: [], flowers: []),
    // ---- 네더 ----
    biome!("nether_wastes", "netherrack", "netherrack", "netherrack", snowy: false,
        trees: [], density: 0, plants: [], flowers: []),
    biome!("soul_sand_valley", "soul_sand", "soul_soil", "soul_sand", snowy: false,
        trees: [], density: 0, plants: [], flowers: []),
    biome!("crimson_forest", "crimson_nylium", "netherrack", "netherrack", snowy: false,
        trees: [], density: 0, plants: [], flowers: []),
    biome!("warped_forest", "warped_nylium", "netherrack", "netherrack", snowy: false,
        trees: [], density: 0, plants: [], flowers: []),
    biome!("basalt_deltas", "basalt", "blackstone", "basalt", snowy: false,
        trees: [], density: 0, plants: [], flowers: []),
    // ---- 엔드 ----
    biome!("the_end", "end_stone", "end_stone", "end_stone", snowy: false,
        trees: [], density: 0, plants: [], flowers: []),
    biome!("end_highlands", "end_stone", "end_stone", "end_stone", snowy: false,
        trees: [], density: 0, plants: [], flowers: []),
    biome!("end_midlands", "end_stone", "end_stone", "end_stone", snowy: false,
        trees: [], density: 0, plants: [], flowers: []),
    biome!("small_end_islands", "end_stone", "end_stone", "end_stone", snowy: false,
        trees: [], density: 0, plants: [], flowers: []),
    biome!("end_barrens", "end_stone", "end_stone", "end_stone", snowy: false,
        trees: [], density: 0, plants: [], flowers: []),
];

fn names() -> &'static HashMap<&'static str, Biome> {
    static N: OnceLock<HashMap<&'static str, Biome>> = OnceLock::new();
    N.get_or_init(|| {
        BIOMES
            .iter()
            .enumerate()
            .map(|(i, d)| (d.name, Biome(i as u8)))
            .collect()
    })
}

/// 이름으로. 없는 이름은 개발 중 실수.
pub fn id(name: &str) -> Biome {
    *names()
        .get(name)
        .unwrap_or_else(|| panic!("unknown biome {name}"))
}

pub fn by_name(name: &str) -> Option<Biome> {
    names()
        .get(name.strip_prefix("minecraft:").unwrap_or(name))
        .copied()
}

impl Biome {
    pub fn def(self) -> &'static BiomeDef {
        &BIOMES[self.0 as usize]
    }

    pub fn name(self) -> &'static str {
        self.def().name
    }

    /// `worldgen/biome` 레지스트리 번호 (청크 패킷에 씀).
    pub fn network_id(self) -> u16 {
        static IDS: OnceLock<Vec<u16>> = OnceLock::new();
        IDS.get_or_init(|| {
            BIOMES
                .iter()
                .map(|d| registry_id("worldgen/biome", d.name) as u16)
                .collect()
        })[self.0 as usize]
    }

    pub fn is_ocean(self) -> bool {
        self.name().ends_with("ocean")
    }
}

// ---- 기후 ----

/// 한 기둥의 기후. 모든 값은 대략 [-1, 1].
#[derive(Debug, Clone, Copy)]
pub struct Climate {
    pub temperature: f64,
    pub humidity: f64,
    /// 바다(-) ↔ 내륙(+).
    pub continentalness: f64,
    /// 낮을수록 험한 산.
    pub erosion: f64,
    /// 같은 기후에서 변종(해바라기 들판, 꽃 숲 등)을 고르는 값.
    pub weirdness: f64,
    /// 0 근처면 강.
    pub river: f64,
}

pub fn climate(seed: u64, x: i32, z: i32) -> Climate {
    let (fx, fz) = (x as f64, z as f64);
    // 큰 규모의 축들. 좌표를 살짝 비틀어(도메인 왜곡) 격자 느낌을 없앱니다.
    let wx = fx + fbm(seed ^ 0xD15, fx, fz, 180.0, 2) * 40.0;
    let wz = fz + fbm(seed ^ 0xD16, fx, fz, 180.0, 2) * 40.0;
    Climate {
        temperature: (fbm(seed ^ 0x7E3, wx, wz, 1400.0, 3) * 1.6).clamp(-1.0, 1.0),
        humidity: (fbm(seed ^ 0x4B1, wx, wz, 1100.0, 3) * 1.6).clamp(-1.0, 1.0),
        continentalness: (fbm(seed ^ 0xC07, wx, wz, 1100.0, 4) * 1.5 + 0.12).clamp(-1.0, 1.0),
        erosion: (fbm(seed ^ 0xE70, wx, wz, 700.0, 3) * 1.5).clamp(-1.0, 1.0),
        weirdness: (fbm(seed ^ 0x3E1, wx, wz, 500.0, 3) * 1.6).clamp(-1.0, 1.0),
        river: fbm(seed ^ 0x51E, wx, wz, 520.0, 3),
    }
}

fn smoothstep(e0: f64, e1: f64, x: f64) -> f64 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

pub const SEA_LEVEL: i32 = 63;

/// 강바닥으로 깎이는 정도 (0 = 강 아님, 1 = 강 한가운데).
pub fn river_factor(c: &Climate) -> f64 {
    let inland = smoothstep(-0.12, 0.0, c.continentalness);
    (1.0 - smoothstep(0.0, 0.045, c.river.abs())) * inland
}

/// 땅 높이 (맨 위 고체 블록의 y). 기후만으로 정해지므로 어느 청크에서 물어도 같습니다.
pub fn surface_height(seed: u64, x: i32, z: i32) -> i32 {
    let c = climate(seed, x, z);
    let (fx, fz) = (x as f64, z as f64);
    let detail = fbm(seed ^ 0xA5A5, fx, fz, 36.0, 3);
    let cont = c.continentalness;

    let ocean_floor = 38.0 + 20.0 * smoothstep(-0.8, -0.2, cont) + detail * 3.0;
    let inland = smoothstep(-0.05, 0.9, cont);
    let mountains = smoothstep(0.15, -0.65, c.erosion) * smoothstep(-0.05, 0.35, cont);
    // 봉우리와 골짜기: 기묘함의 절댓값을 접어 능선을 만듭니다 (바닐라의 PV와 같은 생각).
    let ridge = 1.0 - (3.0 * c.weirdness.abs() - 2.0).abs();
    let ridge = (ridge + 1.0) / 2.0;
    let flatness = smoothstep(-0.2, 0.6, c.erosion);
    let land = 64.0
        + inland * 22.0
        + detail * (7.0 - 4.0 * flatness)
        + fbm(seed ^ 0x5A5A, fx, fz, 90.0, 3) * 6.0 * (1.0 - flatness)
        + mountains * (25.0 + 105.0 * ridge * ridge);
    let land_mix = smoothstep(-0.22, -0.06, cont);
    let mut h = ocean_floor + (land - ocean_floor) * land_mix;

    let r = river_factor(&c);
    if r > 0.0 {
        let bed = (SEA_LEVEL - 4) as f64 + detail;
        // 산을 지나는 강은 덜 깎습니다 (협곡 대신 계곡).
        let strength = r * (1.0 - mountains * 0.7);
        h = h + (bed - h) * strength;
    }
    h.round() as i32
}

/// 지상 바이옴 (기둥의 맨 위 기준).
pub fn surface_biome(seed: u64, x: i32, z: i32, height: i32) -> Biome {
    let c = climate(seed, x, z);
    pick_surface(&c, height)
}

fn pick_surface(c: &Climate, h: i32) -> Biome {
    let (t, hu, w) = (c.temperature, c.humidity, c.weirdness);
    let temp = if t < -0.45 {
        0
    } else if t < -0.15 {
        1
    } else if t < 0.2 {
        2
    } else if t < 0.55 {
        3
    } else {
        4
    };
    let humid = if hu < -0.35 {
        0
    } else if hu < -0.1 {
        1
    } else if hu < 0.1 {
        2
    } else if hu < 0.3 {
        3
    } else {
        4
    };

    if river_factor(c) > 0.55 && h < SEA_LEVEL {
        return id(if temp == 0 { "frozen_river" } else { "river" });
    }
    if h < SEA_LEVEL - 1 {
        if c.continentalness < -0.95 && hu > 0.3 {
            return id("mushroom_fields");
        }
        let deep = h < 45;
        let name = match (temp, deep) {
            (0, false) => "frozen_ocean",
            (0, true) => "deep_frozen_ocean",
            (1, false) => "cold_ocean",
            (1, true) => "deep_cold_ocean",
            (2, false) => "ocean",
            (2, true) => "deep_ocean",
            (3, false) => "lukewarm_ocean",
            (3, true) => "deep_lukewarm_ocean",
            (_, false) => "warm_ocean",
            (_, true) => "deep_lukewarm_ocean",
        };
        return id(name);
    }
    // 해안
    if h <= SEA_LEVEL + 2 && c.continentalness < 0.02 {
        return id(if c.erosion < -0.4 {
            "stony_shore"
        } else if temp == 0 {
            "snowy_beach"
        } else {
            "beach"
        });
    }
    // 높은 산
    if h > 160 {
        return id(if temp <= 1 {
            "frozen_peaks"
        } else if temp == 4 || c.erosion > -0.3 {
            "stony_peaks"
        } else {
            "jagged_peaks"
        });
    }
    if h > 128 {
        return id(if temp <= 2 {
            "snowy_slopes"
        } else {
            "stony_peaks"
        });
    }
    if h > 108 {
        return id(match temp {
            0 | 1 => "grove",
            2 if w > 0.3 => "cherry_grove",
            2 | 3 => "meadow",
            _ => "savanna_plateau",
        });
    }
    // 험한 언덕
    if c.erosion < -0.35 && h > 85 {
        return id(match (temp, humid) {
            (4, _) => "windswept_savanna",
            (_, 0) => "windswept_gravelly_hills",
            (_, 3 | 4) => "windswept_forest",
            _ => "windswept_hills",
        });
    }
    // 늪: 따뜻하고 축축한 저지대
    if (2..=4).contains(&temp) && humid >= 3 && h <= SEA_LEVEL + 3 && c.erosion > 0.3 {
        return id(if temp >= 3 { "mangrove_swamp" } else { "swamp" });
    }
    let name = match temp {
        0 => match humid {
            0 if w > 0.55 => "ice_spikes",
            0..=2 => "snowy_plains",
            _ => "snowy_taiga",
        },
        1 => match humid {
            0 | 1 => "plains",
            2 => "forest",
            3 => "taiga",
            _ if w > 0.2 => "old_growth_pine_taiga",
            _ => "old_growth_spruce_taiga",
        },
        2 => match humid {
            0 if w > 0.5 => "sunflower_plains",
            0 => "plains",
            1 if w < -0.4 => "flower_forest",
            1 => "plains",
            2 if w > 0.35 => "dappled_forest",
            2 => "forest",
            3 if w > 0.4 => "old_growth_birch_forest",
            3 => "birch_forest",
            _ if w < -0.45 => "pale_garden",
            _ => "dark_forest",
        },
        3 => match humid {
            0 | 1 => "savanna",
            2 if w > 0.2 => "dappled_forest",
            2 => "forest",
            3 => "sparse_jungle",
            _ if w > 0.5 => "bamboo_jungle",
            _ => "jungle",
        },
        _ => match humid {
            0 | 1 => "desert",
            2 if c.erosion < 0.0 => "eroded_badlands",
            2 => "badlands",
            3 => "wooded_badlands",
            _ => "jungle",
        },
    };
    id(name)
}

/// 동굴 바이옴: 지표에서 충분히 아래이고 해당 노이즈 지역이면.
pub fn cave_biome_with(
    seed: u64,
    c: &Climate,
    x: i32,
    y: i32,
    z: i32,
    surface: i32,
) -> Option<Biome> {
    if y > surface - 24 {
        return None;
    }
    let region = fbm(seed ^ 0xCA7E, x as f64, z as f64, 260.0, 2);
    let layer = fbm(seed ^ 0xCA7F, x as f64 + y as f64 * 3.0, z as f64, 120.0, 2);
    if y < 0 && c.erosion < -0.25 && c.continentalness > 0.25 && region > 0.25 {
        return Some(id("deep_dark"));
    }
    if region > 0.35 && c.temperature > 0.15 && c.humidity < 0.05 && layer > -0.2 {
        return Some(id("sulfur_caves"));
    }
    if region < -0.35 && c.humidity > 0.2 {
        return Some(id("lush_caves"));
    }
    if c.continentalness > 0.55 && region.abs() < 0.15 {
        return Some(id("dripstone_caves"));
    }
    None
}

pub fn nether_biome(seed: u64, x: i32, z: i32) -> Biome {
    let t = fbm(seed ^ 0x2E7, x as f64, z as f64, 220.0, 3);
    let h = fbm(seed ^ 0x2E8, x as f64, z as f64, 220.0, 3);
    id(if t > 0.3 {
        "basalt_deltas"
    } else if t < -0.3 {
        "soul_sand_valley"
    } else if h > 0.25 {
        "crimson_forest"
    } else if h < -0.25 {
        "warped_forest"
    } else {
        "nether_wastes"
    })
}

/// 지상 바이옴 이름 목록 (`/locate biome` 자동 완성 등).
pub fn all() -> impl Iterator<Item = Biome> {
    (0..BIOMES.len()).map(|i| Biome(i as u8))
}

/// 결정적 난수 (청크·용도마다).
pub fn chunk_rng(seed: u64, cx: i32, cz: i32, salt: u64) -> super::features::Rng {
    super::features::Rng::new(hash(seed ^ salt, cx, cz))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_biome_is_in_the_registry() {
        for b in all() {
            b.network_id();
        }
    }

    #[test]
    fn world_has_variety() {
        use std::collections::HashSet;
        let mut seen = HashSet::new();
        for i in 0..2000 {
            let (x, z) = ((i % 50) * 400 - 10000, (i / 50) * 500 - 10000);
            let h = surface_height(7, x, z);
            seen.insert(surface_biome(7, x, z, h).name());
        }
        for must in [
            "plains",
            "desert",
            "ocean",
            "forest",
            "taiga",
            "snowy_plains",
            "jungle",
        ] {
            assert!(seen.contains(must), "{must} missing: {seen:?}");
        }
        assert!(seen.len() >= 20, "{} biomes: {seen:?}", seen.len());
    }

    #[test]
    fn heights_are_sane() {
        for i in 0..500 {
            let h = surface_height(3, i * 97 - 20000, i * 61 - 15000);
            assert!((20..=300).contains(&h), "{h}");
        }
    }
}
