//! 청크 기둥: 섹션(16×16×16)마다 블록 상태와 바이옴(4×4×4 칸).
//!
//! 섹션은 한 가지 블록뿐이면 그 값 하나만 들고(공기·돌로 꽉 찬 섹션이 많음), 아니면 4096칸
//! 전부를 u16으로 듭니다 (블록 상태는 35723가지라 16비트에 들어감). 그래서 메모리에 청크를
//! 많이 둘 수 있습니다.

use super::Dimension;
use super::biome::Biome;
use crate::data::block;

#[derive(Clone)]
pub enum Section {
    Uniform(u16),
    Full(Box<[u16; 4096]>),
}

/// 섹션을 읽기 전용으로.
pub enum SectionView<'a> {
    Uniform(u32),
    Full(&'a [u16; 4096]),
}

impl SectionView<'_> {
    pub fn get(&self, i: usize) -> u32 {
        match self {
            SectionView::Uniform(s) => *s,
            SectionView::Full(a) => a[i] as u32,
        }
    }
}

#[derive(Clone)]
pub struct Column {
    pub dimension: Dimension,
    sections: Vec<Section>,
    /// (높이/4) × 16칸, `ycell*16 + (z/4)*4 + x/4`. 값은 [`Biome`] 번호.
    biomes: Vec<u8>,
    /// 구조물 상자들의 전리품 표: (월드 좌표, 표 이름).
    pub loot: Vec<((i32, i32, i32), &'static str)>,
    /// 몹 생성기: (월드 좌표, 엔티티 이름).
    pub spawners: Vec<((i32, i32, i32), &'static str)>,
}

impl Column {
    pub fn new(dimension: Dimension, fill_biome: Biome) -> Self {
        Self {
            dimension,
            sections: vec![Section::Uniform(block::AIR as u16); dimension.section_count()],
            biomes: vec![fill_biome.0; dimension.height() as usize / 4 * 16],
            loot: Vec::new(),
            spawners: Vec::new(),
        }
    }

    fn locate(&self, y: i32) -> Option<(usize, usize)> {
        let ry = y - self.dimension.min_y();
        (0..self.dimension.height())
            .contains(&ry)
            .then_some(((ry / 16) as usize, (ry % 16) as usize))
    }

    pub fn get(&self, x: usize, y: i32, z: usize) -> u32 {
        let Some((s, ly)) = self.locate(y) else {
            return block::AIR;
        };
        match &self.sections[s] {
            Section::Uniform(v) => *v as u32,
            Section::Full(a) => a[ly * 256 + z * 16 + x] as u32,
        }
    }

    pub fn set(&mut self, x: usize, y: i32, z: usize, state: u32) {
        let Some((s, ly)) = self.locate(y) else {
            return;
        };
        let i = ly * 256 + z * 16 + x;
        let sec = &mut self.sections[s];
        if let Section::Uniform(v) = *sec {
            if v as u32 == state {
                return;
            }
            *sec = Section::Full(Box::new([v; 4096]));
        }
        if let Section::Full(a) = sec {
            a[i] = state as u16;
        }
    }

    /// 섹션 하나(아래에서 `section`번째), `y*256 + z*16 + x` 순서.
    pub fn section(&self, section: usize) -> SectionView<'_> {
        match &self.sections[section] {
            Section::Uniform(v) => SectionView::Uniform(*v as u32),
            Section::Full(a) => SectionView::Full(a),
        }
    }

    /// 섹션 하나를 통째로 바꿈 (월드 파일에서 읽을 때).
    pub fn put_section(&mut self, section: usize, blocks: &[u16]) {
        if section >= self.sections.len() || blocks.len() != 4096 {
            return;
        }
        let first = blocks[0];
        self.sections[section] = if blocks.iter().all(|&b| b == first) {
            Section::Uniform(first)
        } else {
            let mut a = Box::new([0u16; 4096]);
            a.copy_from_slice(blocks);
            Section::Full(a)
        };
    }

    /// 다 같은 블록으로 된 섹션을 다시 하나로 접습니다 (생성 뒤 메모리 절약).
    pub fn compact(&mut self) {
        for sec in &mut self.sections {
            if let Section::Full(a) = sec {
                let first = a[0];
                if a.iter().all(|&b| b == first) {
                    *sec = Section::Uniform(first);
                }
            }
        }
    }

    /// 섹션 하나의 바이옴 64칸, `(y*4 + z)*4 + x` 순서 (바닐라 팔레트 순서).
    pub fn section_biomes(&self, section: usize) -> &[u8] {
        &self.biomes[section * 64..(section + 1) * 64]
    }

    pub fn set_section_biomes(&mut self, section: usize, cells: &[Biome]) {
        for (i, b) in cells.iter().enumerate().take(64) {
            if let Some(c) = self.biomes.get_mut(section * 64 + i) {
                *c = b.0;
            }
        }
    }

    pub fn biome_at(&self, x: usize, y: i32, z: usize) -> Biome {
        let ry = ((y - self.dimension.min_y()).clamp(0, self.dimension.height() - 1) / 4) as usize;
        Biome(self.biomes[ry * 16 + (z / 4) * 4 + x / 4])
    }

    pub fn set_biome_cell(&mut self, cx: usize, ycell: usize, cz: usize, b: Biome) {
        if let Some(c) = self.biomes.get_mut(ycell * 16 + cz * 4 + cx) {
            *c = b.0;
        }
    }

    /// 메모리 크기 어림 (바이트).
    pub fn memory(&self) -> usize {
        self.sections
            .iter()
            .map(|s| match s {
                Section::Uniform(_) => 2,
                Section::Full(_) => 8192,
            })
            .sum::<usize>()
            + self.biomes.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::biome;

    #[test]
    fn uniform_sections_expand_and_compact() {
        let mut c = Column::new(Dimension::Overworld, biome::id("plains"));
        assert_eq!(c.get(3, 10, 4), block::AIR);
        let before = c.memory();
        c.set(3, 10, 4, block::STONE);
        assert_eq!(c.get(3, 10, 4), block::STONE);
        assert!(c.memory() > before);
        c.set(3, 10, 4, block::AIR);
        c.compact();
        assert_eq!(c.memory(), before);
        assert_eq!(c.get(0, 1000, 0), block::AIR);
    }
}
