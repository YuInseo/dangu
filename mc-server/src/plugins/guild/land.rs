//! 길드 땅: 청크(16×16, 높이 전부) 단위로 사고팝니다.
//!
//! - 길드 땅에서는 길드원과 방문객만 블록을 부수고·놓고·상자·문을 씁니다.
//! - 들어가고 나갈 때 그 길드의 입장·퇴장 메시지가 화면에 뜹니다.
//! - 판매하면 산 값의 절반을 돌려받습니다.
//! - 길드 땅의 작물은 업그레이드만큼 빨리 자랍니다 ([`super::Guild::growth_multiplier`]).

use super::*;

/// 청크 하나의 값.
pub const PRICE: i64 = 10_000;

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
                paid: PRICE,
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
            let refund = l.paid / 2;
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

/// 이 청크 작물의 성장 배율 (길드 땅이 아니면 1).
pub fn growth_at(instance: &str, dimension: Dimension, chunk: (i32, i32)) -> f64 {
    guilds().read(|d| {
        d.owner_of(instance, dimension, chunk)
            .map_or(1.0, Guild::growth_multiplier)
    })
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
        let (_, refund) = d.sell(&a, 0).unwrap();
        assert_eq!(refund, PRICE / 2);
        assert_eq!(chunk_of(-0.5, 17.0), (-1, 1));
    }
}
