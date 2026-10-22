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

/// 3차원 정수 좌표 → [0, 1).
pub fn hash3_01(seed: u64, x: i32, y: i32, z: i32) -> f64 {
    let h = hash(
        seed ^ (y as u32 as u64).wrapping_mul(0xD6E8_FEB8_6659_FD93),
        x,
        z,
    );
    (h >> 11) as f64 / (1u64 << 53) as f64
}

/// 3차원 값 노이즈, 결과 [-1, 1). 축마다 격자 간격이 다를 수 있습니다 (동굴은 납작하게).
pub fn value3(seed: u64, x: f64, y: f64, z: f64, sxz: f64, sy: f64) -> f64 {
    let (fx, fy, fz) = (x / sxz, y / sy, z / sxz);
    let (x0, y0, z0) = (fx.floor(), fy.floor(), fz.floor());
    let (tx, ty, tz) = (smooth(fx - x0), smooth(fy - y0), smooth(fz - z0));
    let (ix, iy, iz) = (x0 as i32, y0 as i32, z0 as i32);
    let c = |dx, dy, dz| hash3_01(seed, ix + dx, iy + dy, iz + dz);
    let lerp = |a: f64, b: f64, t: f64| a + (b - a) * t;
    let x00 = lerp(c(0, 0, 0), c(1, 0, 0), tx);
    let x10 = lerp(c(0, 1, 0), c(1, 1, 0), tx);
    let x01 = lerp(c(0, 0, 1), c(1, 0, 1), tx);
    let x11 = lerp(c(0, 1, 1), c(1, 1, 1), tx);
    let y0v = lerp(x00, x10, ty);
    let y1v = lerp(x01, x11, ty);
    lerp(y0v, y1v, tz) * 2.0 - 1.0
}

/// 옥타브 두 개짜리 3차원 노이즈.
pub fn fbm3(seed: u64, x: f64, y: f64, z: f64, sxz: f64, sy: f64) -> f64 {
    (value3(seed, x, y, z, sxz, sy) * 2.0 + value3(seed ^ 0x51, x, y, z, sxz / 2.0, sy / 2.0)) / 3.0
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
