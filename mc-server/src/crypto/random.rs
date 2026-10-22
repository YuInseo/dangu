//! 운영체제 난수 (`/dev/urandom`). 키와 검증 토큰에 씁니다.

use std::io::Read;

pub fn fill(buf: &mut [u8]) {
    std::fs::File::open("/dev/urandom")
        .and_then(|mut f| f.read_exact(buf))
        .expect("/dev/urandom을 읽을 수 없습니다");
}

pub fn bytes<const N: usize>() -> [u8; N] {
    let mut b = [0u8; N];
    fill(&mut b);
    b
}
