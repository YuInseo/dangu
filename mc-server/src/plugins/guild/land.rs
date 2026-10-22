//! 길드 땅: 청크(16×16, 높이 전부) 단위로 사고팝니다.
//!
//! - 길드 땅에서는 길드원과 방문객만 블록을 부수고·놓고·상자·문을 씁니다.
//! - 들어가고 나갈 때 그 길드의 입장·퇴장 메시지가 화면에 뜹니다.
//! - 판매하면 산 값의 일부(설정 `land_refund`)를 돌려받습니다.
//! - 작물 성장은 청크마다: 작물 성장 촉진제를 길드 청크 지도에서 그 청크에 쓰면 단계가 오르고,
//!   그 청크의 작물만 설정 `crop_levels`의 배율로 빨리 자랍니다.

use super::*;

/// 청크 하나의 값 (설정).
pub fn price() -> i64 {
    super::config::with(|c| c.land_price)
}

pub fn chunk_of(x: f64, z: f64) -> (i32, i32) {
    ((x.floor() as i32) >> 4, (z.floor() as i32) >> 4)
}

impl Land {
    pub fn is(&self, instance: &str, dimension: Dimension, chunk: (i32, i32)) -> bool {
        self.chunk == chunk && self.dimension == dimension && self.instance == instance
    }

    /// 청크 가운데의 블록 좌표.
    pub fn center(&self) -> (i32, i32) {
        (self.chunk.0 * 16 + 8, self.chunk.1 * 16 + 8)
    }
}

impl Data {
    /// 이 청크를 가진 길드.
    pub fn owner_of(
        &self,
        instance: &str,
        dimension: Dimension,
        chunk: (i32, i32),
    ) -> Option<&Guild> {
        self.guilds
            .iter()
            .find(|g| g.lands.iter().any(|l| l.is(instance, dimension, chunk)))
    }

    /// 여기서 짓고 부숴도 되나. 주인 없는 땅이면 누구나.
    pub fn may_build(
        &self,
        uuid: u128,
        instance: &str,
        dimension: Dimension,
        chunk: (i32, i32),
    ) -> bool {
        match self.owner_of(instance, dimension, chunk) {
            None => true,
            Some(g) => g.member(uuid).is_some() || g.visitors.iter().any(|v| v.0 == uuid),
        }
    }

    /// 땅 사기 (돈은 부른 쪽이 먼저 받아 둠 — 실패하면 돌려주기).
    pub fn claim(
        &mut self,
        actor: &Actor,
        instance: &str,
        dimension: Dimension,
        chunk: (i32, i32),
        now: i64,
    ) -> Outcome {
        if let Some(owner) = self.owner_of(instance, dimension, chunk) {
            return Err(format!("이미 {} 길드의 땅입니다.", owner.name));
        }
        self.with_perm(actor, Some(Perm::Land), |g| {
            if g.lands.len() >= g.max_lands() {
                return Err(format!(
                    "땅은 {}청크까지입니다 (업그레이드로 늘리기).",
                    g.max_lands()
                ));
            }
            // 처음 땅이 아니면 이미 가진 땅에 붙어 있어야 합니다
            let touching = g.lands.iter().any(|l| {
                l.instance == instance
                    && l.dimension == dimension
                    && (l.chunk.0 - chunk.0).abs() + (l.chunk.1 - chunk.1).abs() == 1
            });
            if !g.lands.is_empty() && !touching {
                return Err("길드 땅에 붙어 있는 청크만 살 수 있습니다.".into());
            }
            g.lands.push(Land {
                instance: instance.to_owned(),
                dimension,
                chunk,
                bought: now,
                paid: price(),
                crop_level: 0,
            });
            Ok(format!(
                "청크 ({}, {})를 샀습니다. ({}/{})",
                chunk.0,
                chunk.1,
                g.lands.len(),
                g.max_lands()
            ))
        })
    }

    /// 땅 팔기: 돌려줄 돈과 함께.
    pub fn sell(&mut self, actor: &Actor, index: usize) -> Result<(String, i64), String> {
        self.with_perm(actor, Some(Perm::Land), |g| {
            if index >= g.lands.len() {
                return Err("그 땅이 없습니다.".into());
            }
            let l = g.lands.remove(index);
            let refund = (l.paid as f64 * super::config::with(|c| c.land_refund)) as i64;
            Ok((
                format!(
                    "청크 ({}, {})를 팔았습니다. {}원 돌려받음",
                    l.chunk.0, l.chunk.1, refund
                ),
                refund,
            ))
        })
    }
}

/// 이 청크 작물의 성장 배율 (길드 땅이 아니거나 0단계면 1).
pub fn growth_at(instance: &str, dimension: Dimension, chunk: (i32, i32)) -> f64 {
    let level = guilds().read(|d| {
        d.owner_of(instance, dimension, chunk)
            .and_then(|g| g.lands.iter().find(|l| l.is(instance, dimension, chunk)))
            .map_or(0, |l| l.crop_level)
    });
    if level == 0 {
        1.0
    } else {
        super::config::with(|c| c.crop_multiplier(level))
    }
}

impl Data {
    /// 우리 길드 청크의 작물 단계를 하나 올림 (촉진제는 부른 쪽이 먼저 받아 둠).
    pub fn crop_upgrade(
        &mut self,
        actor: &Actor,
        instance: &str,
        dimension: Dimension,
        chunk: (i32, i32),
    ) -> Outcome {
        let max = super::config::with(|c| c.max_crop_level());
        self.with_perm(actor, Some(Perm::Land), |g| {
            let l = g
                .lands
                .iter_mut()
                .find(|l| l.is(instance, dimension, chunk))
                .ok_or("우리 길드 청크가 아닙니다.")?;
            if l.crop_level >= max {
                return Err(format!("이미 최고 단계({max})입니다."));
            }
            l.crop_level += 1;
            let mult = super::config::with(|c| c.crop_multiplier(l.crop_level));
            Ok(format!(
                "청크 ({}, {}) 작물 성장 {}단계 (×{mult:.2})",
                chunk.0, chunk.1, l.crop_level
            ))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claims_must_touch_and_protect() {
        let mut d = Data::default();
        let a = Actor {
            uuid: 1,
            name: "A".into(),
        };
        d.create(&a, "길드", 0).unwrap();
        let o = Dimension::Overworld;
        d.claim(&a, "main", o, (0, 0), 0).unwrap();
        assert!(d.claim(&a, "main", o, (5, 5), 0).is_err());
        d.claim(&a, "main", o, (1, 0), 0).unwrap();
        assert!(d.claim(&a, "main", o, (1, 0), 0).is_err());
        assert!(d.may_build(1, "main", o, (1, 0)));
        assert!(!d.may_build(2, "main", o, (1, 0)));
        assert!(d.may_build(2, "main", o, (9, 9)));
        assert!(d.may_build(2, "other", o, (1, 0)), "다른 인스턴스");
        d.visitor(&a, (2, "B".into()), true).unwrap();
        assert!(d.may_build(2, "main", o, (1, 0)));
        // 청크마다 작물 단계
        d.crop_upgrade(&a, "main", o, (1, 0)).unwrap();
        assert!(d.crop_upgrade(&a, "main", o, (7, 7)).is_err());
        assert_eq!(d.of(1).unwrap().lands[1].crop_level, 1);
        assert_eq!(d.of(1).unwrap().lands[0].crop_level, 0);
        let (_, refund) = d.sell(&a, 0).unwrap();
        assert_eq!(refund, price() / 2);
        assert_eq!(chunk_of(-0.5, 17.0), (-1, 1));
    }
}
