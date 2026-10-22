//! 시드 기반 2D 값 노이즈. 지형 높이를 부드럽게 만드는 데만 씁니다.

/// 정수 좌표 → [0, 1) 난수. 같은 입력이면 항상 같은 값.
pub fn hash01(seed: u64, x: i32, z: i32) -> f64 {
    (hash(seed, x, z) >> 11) as f64 / (1u64 << 53) as f64
}

/// splitmix64 계열의 정수 섞기.
pub fn hash(seed: u64, x: i32, z: i32) -> u64 {
    let mut h = seed
        ^ (x as u32 as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15)
        ^ (z as u32 as u64).wrapping_mul(0xC2B2_AE3D_27D4_EB4F);
    h ^= h >> 30;
    h = h.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    h ^= h >> 27;
    h = h.wrapping_mul(0x94D0_49BB_1331_11EB);
    h ^ (h >> 31)
}

fn smooth(t: f64) -> f64 {
    t * t * (3.0 - 2.0 * t)
}

/// 격자 간격 `scale`의 값 노이즈, 결과 [0, 1).
pub fn value(seed: u64, x: f64, z: f64, scale: f64) -> f64 {
    let (fx, fz) = (x / scale, z / scale);
    let (x0, z0) = (fx.floor(), fz.floor());
    let (tx, tz) = (smooth(fx - x0), smooth(fz - z0));
    let (ix, iz) = (x0 as i32, z0 as i32);
    let a = hash01(seed, ix, iz);
    let b = hash01(seed, ix + 1, iz);
    let c = hash01(seed, ix, iz + 1);
    let d = hash01(seed, ix + 1, iz + 1);
    let top = a + (b - a) * tx;
    let bottom = c + (d - c) * tx;
    top + (bottom - top) * tz
}

/// 옥타브를 겹친 노이즈, 결과 대략 [-1, 1].
pub fn fbm(seed: u64, x: f64, z: f64, scale: f64, octaves: u32) -> f64 {
    let (mut sum, mut amp, mut norm, mut s) = (0.0, 1.0, 0.0, scale);
    for o in 0..octaves {
        sum += (value(seed.wrapping_add(o as u64 * 1013), x, z, s) * 2.0 - 1.0) * amp;
        norm += amp;
        amp *= 0.5;
        s *= 0.5;
    }
    sum / norm
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic_and_bounded() {
        for i in -50..50 {
            let v = fbm(42, i as f64 * 3.7, i as f64 * -1.3, 64.0, 4);
            assert_eq!(v, fbm(42, i as f64 * 3.7, i as f64 * -1.3, 64.0, 4));
            assert!((-1.0..=1.0).contains(&v));
        }
    }
}
