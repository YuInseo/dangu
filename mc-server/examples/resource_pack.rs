//! 서버가 내려 주는 리소스 팩을 파일로 씁니다 (그림 확인용).
//! `cargo run --example resource_pack -- hoya.zip`

fn main() {
    let out = std::env::args().nth(1).unwrap_or_else(|| "hoya.zip".into());
    let p = mc_server::resource_pack::pack();
    std::fs::write(&out, &p.bytes).unwrap();
    println!("{out}: {} bytes, sha1 {}", p.bytes.len(), p.sha1_hex);
}
