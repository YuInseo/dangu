//! 세 차원(오버월드·네더·엔드)과 청크.
//!
//! 월드는 저장하지 않고, 같은 시드로 언제든 같은 지형을 다시 만듭니다. 그래서 청크를
//! 보낼 때마다 그 자리에서 생성합니다 — 블록을 바꾸는 기능이 생기기 전까지는 이걸로 충분합니다.

pub mod biome;
pub mod blockset;
pub mod chunk;
pub mod features;
pub mod level;
mod noise;
pub mod placement;
pub mod store;
pub mod structure;
pub mod terrain;

/// 좌표 섞기 (결정적 난수 씨앗).
pub fn noise_hash(seed: u64, x: i32, z: i32) -> u64 {
    noise::hash(seed, x, z)
}

use crate::data::registry_id;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Dimension {
    Overworld,
    Nether,
    End,
}

impl Dimension {
    pub const ALL: [Dimension; 3] = [Dimension::Overworld, Dimension::Nether, Dimension::End];

    /// 월드(레벨) 이름. Login 패킷의 월드 목록과 Respawn에 씁니다.
    pub fn level_name(self) -> &'static str {
        match self {
            Dimension::Overworld => "minecraft:overworld",
            Dimension::Nether => "minecraft:the_nether",
            Dimension::End => "minecraft:the_end",
        }
    }

    /// `dimension_type` 레지스트리의 항목 이름.
    fn type_key(self) -> &'static str {
        match self {
            Dimension::Overworld => "overworld",
            Dimension::Nether => "the_nether",
            Dimension::End => "the_end",
        }
    }

    /// `dimension_type` 레지스트리 안의 네트워크 ID.
    pub fn type_id(self) -> i32 {
        registry_id("dimension_type", self.type_key())
    }

    /// 이 차원 전체를 덮는 바이옴의 네트워크 ID.
    pub fn biome_id(self) -> i32 {
        let biome = match self {
            Dimension::Overworld => "plains",
            Dimension::Nether => "nether_wastes",
            Dimension::End => "the_end",
        };
        registry_id("worldgen/biome", biome)
    }

    /// 아래 값들은 바닐라 `dimension_type` 정의와 같아야 합니다. 클라이언트는 자기 데이터로
    /// 높이를 정하므로, 다르면 청크 섹션 수가 어긋나 접속이 끊깁니다.
    pub fn min_y(self) -> i32 {
        match self {
            Dimension::Overworld => -64,
            Dimension::Nether | Dimension::End => 0,
        }
    }

    pub fn height(self) -> i32 {
        match self {
            Dimension::Overworld => 384,
            Dimension::Nether | Dimension::End => 256,
        }
    }

    pub fn section_count(self) -> usize {
        (self.height() / 16) as usize
    }

    /// 26.x에서 엔드도 하늘빛을 갖게 되었습니다. 네더만 없습니다.
    pub fn has_skylight(self) -> bool {
        !matches!(self, Dimension::Nether)
    }

    pub fn sea_level(self) -> i32 {
        match self {
            Dimension::Overworld => terrain::SEA_LEVEL,
            Dimension::Nether => terrain::LAVA_LEVEL,
            Dimension::End => 0,
        }
    }

    /// 명령어 이름으로 찾습니다.
    pub fn from_command(name: &str) -> Option<Dimension> {
        match name {
            "overworld" => Some(Dimension::Overworld),
            "nether" => Some(Dimension::Nether),
            "end" => Some(Dimension::End),
            _ => None,
        }
    }

    pub fn display_name(self) -> &'static str {
        match self {
            Dimension::Overworld => "오버월드",
            Dimension::Nether => "네더",
            Dimension::End => "엔드",
        }
    }
}

impl Dimension {
    /// 저장 파일에 쓰는 번호.
    pub fn index(self) -> u8 {
        match self {
            Dimension::Overworld => 0,
            Dimension::Nether => 1,
            Dimension::End => 2,
        }
    }

    pub fn from_index(i: u8) -> Option<Dimension> {
        Dimension::ALL.get(i as usize).copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn heights_are_whole_sections() {
        for d in Dimension::ALL {
            assert_eq!(d.height() % 16, 0);
            assert_eq!(d.min_y() % 16, 0);
            d.type_id();
            d.biome_id();
        }
    }
}
