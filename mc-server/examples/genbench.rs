//! 청크 생성 속도와 구조물 분포를 재 봅니다: `cargo run --release --example genbench [시드]`
use std::time::Instant;

use mc_server::world::{Dimension, chunk, structure, terrain};

fn main() {
    let seed: u64 = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(20260915);
    for dim in Dimension::ALL {
        let t = Instant::now();
        let mut bytes = 0;
        let n = 12;
        for cx in -n / 2..n / 2 {
            for cz in -n / 2..n / 2 {
                let col = terrain::generate(dim, seed, cx, cz);
                bytes += chunk::encode(&col, cx, cz).len();
            }
        }
        let per = t.elapsed().as_secs_f64() * 1000.0 / (n * n) as f64;
        println!(
            "{dim:?}: {per:.2} ms/청크, 평균 {} 바이트",
            bytes / (n * n) as usize
        );
    }
    for name in [
        "village",
        "desert_pyramid",
        "stronghold",
        "mineshaft",
        "abandoned_camp",
        "igloo",
        "ruined_portal",
        "trial_chambers",
        "monument",
        "mansion",
    ] {
        match structure::locate(seed, Dimension::Overworld, name, (0, 0), 600) {
            Some((x, z, s)) => println!("{name:>16}: {s} at {x} {z}"),
            None => println!("{name:>16}: 없음"),
        }
    }
    for name in ["fortress", "bastion_remnant"] {
        println!(
            "{name:>16}: {:?}",
            structure::locate(seed, Dimension::Nether, name, (0, 0), 200)
        );
    }
    println!(
        "{:>16}: {:?}",
        "end_city",
        structure::locate(seed, Dimension::End, "end_city", (1500, 0), 200)
    );
}
