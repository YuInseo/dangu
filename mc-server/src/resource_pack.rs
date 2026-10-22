//! 서버가 직접 만들어 내려 주는 리소스 팩: 우리만의 아이템 그림(도트).
//!
//! 그림은 아래 문자열 격자로 그려 두고, 켤 때 PNG(직접 인코딩: zlib + CRC32)로 굽고, 무압축
//! ZIP으로 묶어 내장 HTTP 서버로 내려 줍니다. 클라이언트에게는 Configuration 단계에서
//! `Add Resource Pack`(주소 + SHA-1)으로 알립니다. 아이템은 `item_model`이
//! `dangu:<이름>`을 가리키면 이 팩의 그림으로 보입니다.
//!
//! 팩 내용은 코드에서 정해지므로 어느 프로세스에서 만들어도 같은 바이트·같은 해시입니다
//! (담비 마스터가 내려 주고, 자식 인스턴스는 주소만 알려 줘도 됨).

use std::sync::{Arc, OnceLock, RwLock};

use crate::protocol::zlib;

/// 26.3의 리소스 팩 판 (`version.json`의 `resource_pack_version` 97.1).
pub const PACK_FORMAT: (u32, u32) = (97, 1);

/// 그림 하나: 이름(`dangu:item/<이름>`), 16×16 격자, 팔레트.
pub struct Sprite {
    pub name: &'static str,
    pub rows: [&'static str; 16],
    pub palette: &'static [(char, [u8; 4])],
}

impl Sprite {
    pub fn rgba(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(16 * 16 * 4);
        for row in self.rows {
            for ch in row.chars() {
                let px = self
                    .palette
                    .iter()
                    .find(|(c, _)| *c == ch)
                    .map_or([0xff, 0, 0xff, 0xff], |(_, p)| *p);
                out.extend_from_slice(&px);
            }
        }
        out
    }
}

/// RGBA 이미지를 PNG로.
pub fn png(width: u32, height: u32, rgba: &[u8]) -> Vec<u8> {
    fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
        out.extend_from_slice(&(data.len() as u32).to_be_bytes());
        let mut body = kind.to_vec();
        body.extend_from_slice(data);
        out.extend_from_slice(&body);
        out.extend_from_slice(&zlib::crc32(&body).to_be_bytes());
    }
    let mut out = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&width.to_be_bytes());
    ihdr.extend_from_slice(&height.to_be_bytes());
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]); // 8비트 RGBA
    chunk(&mut out, b"IHDR", &ihdr);
    let mut raw = Vec::with_capacity(rgba.len() + height as usize);
    for row in rgba.chunks(width as usize * 4) {
        raw.push(0); // 필터 없음
        raw.extend_from_slice(row);
    }
    chunk(&mut out, b"IDAT", &zlib::compress(&raw));
    chunk(&mut out, b"IEND", &[]);
    out
}

/// 무압축(stored) ZIP. 날짜는 1980-01-01로 고정해 같은 내용이면 같은 바이트.
pub fn zip(files: &[(String, Vec<u8>)]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut central = Vec::new();
    for (name, data) in files {
        let offset = out.len() as u32;
        let crc = zlib::crc32(data);
        let header = |sig: u32, central: bool| {
            let mut h = Vec::new();
            h.extend_from_slice(&sig.to_le_bytes());
            if central {
                h.extend_from_slice(&20u16.to_le_bytes()); // 만든 판
            }
            h.extend_from_slice(&20u16.to_le_bytes()); // 필요한 판
            h.extend_from_slice(&0u16.to_le_bytes()); // 플래그
            h.extend_from_slice(&0u16.to_le_bytes()); // 무압축
            h.extend_from_slice(&0u16.to_le_bytes()); // 시각
            h.extend_from_slice(&0x21u16.to_le_bytes()); // 1980-01-01
            h.extend_from_slice(&crc.to_le_bytes());
            h.extend_from_slice(&(data.len() as u32).to_le_bytes());
            h.extend_from_slice(&(data.len() as u32).to_le_bytes());
            h.extend_from_slice(&(name.len() as u16).to_le_bytes());
            h.extend_from_slice(&0u16.to_le_bytes()); // 덧붙임
            if central {
                h.extend_from_slice(&0u16.to_le_bytes()); // 설명
                h.extend_from_slice(&0u16.to_le_bytes()); // 디스크
                h.extend_from_slice(&0u16.to_le_bytes()); // 내부 속성
                h.extend_from_slice(&0u32.to_le_bytes()); // 외부 속성
                h.extend_from_slice(&offset.to_le_bytes());
            }
            h.extend_from_slice(name.as_bytes());
            h
        };
        out.extend_from_slice(&header(0x0403_4b50, false));
        out.extend_from_slice(data);
        central.extend_from_slice(&header(0x0201_4b50, true));
    }
    let cd_offset = out.len() as u32;
    out.extend_from_slice(&central);
    out.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
    out.extend_from_slice(&[0, 0, 0, 0]);
    out.extend_from_slice(&(files.len() as u16).to_le_bytes());
    out.extend_from_slice(&(files.len() as u16).to_le_bytes());
    out.extend_from_slice(&(central.len() as u32).to_le_bytes());
    out.extend_from_slice(&cd_offset.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out
}

pub struct Pack {
    pub bytes: Vec<u8>,
    pub sha1_hex: String,
    pub uuid: u128,
}

/// 팩의 UUID는 고정입니다: 다시 만들어도 같은 UUID로 보내면 클라이언트가 바꿔 끼웁니다.
pub const PACK_UUID: u128 = 0x6461_6e67_7550_4000_8000_7061_636b_0001;

static PACK: RwLock<Option<Arc<Pack>>> = RwLock::new(None);

/// 지금 팩 (처음이면 만듦).
pub fn pack() -> Arc<Pack> {
    if let Some(p) = PACK.read().unwrap().as_ref() {
        return Arc::clone(p);
    }
    rebuild()
}

/// 콘텐츠([`crate::plugins::contents`])를 다시 읽은 뒤 등 — 팩을 새로 만듭니다.
pub fn rebuild() -> Arc<Pack> {
    let p = Arc::new(build());
    *PACK.write().unwrap() = Some(Arc::clone(&p));
    p
}

fn build() -> Pack {
    let mut files = vec![(
        "pack.mcmeta".to_owned(),
        format!(
            r#"{{"pack":{{"description":"담비 서버 아이템","pack_format":{0},"min_format":[{0},0],"max_format":[{0},{1}]}}}}"#,
            PACK_FORMAT.0, PACK_FORMAT.1
        )
        .into_bytes(),
    )];
    // ItemsAdder식 콘텐츠의 그림·모델·폰트·블록 상태 (같은 이름이면 콘텐츠가 이김)
    for (name, bytes) in crate::plugins::contents::current().files.iter() {
        if name == "pack.mcmeta" {
            continue;
        }
        files.retain(|(n, _)| n != name);
        files.push((name.clone(), bytes.clone()));
    }
    let bytes = zip(&files);
    let hash = crate::crypto::sha1::sha1(&bytes);
    let sha1_hex: String = hash.iter().map(|b| format!("{b:02x}")).collect();
    Pack {
        bytes,
        sha1_hex,
        uuid: PACK_UUID,
    }
}

struct Settings {
    port: u16,
    url: Option<String>,
    forced: bool,
}

/// 팩 주소에 게임 포트를 쓸까 (`MC_PACK_VIA_GAME_PORT`, 기본 켬). 호스팅에서 게임 포트 하나만 열어 둔
/// 곳이 많아서입니다. 게임 포트 앞에 마인크래프트 전용 프록시(Velocity·BungeeCord·TCPShield 등)가
/// 있으면 HTTP가 지나가지 못하므로 끄거나 `MC_PACK_URL`을 정하세요.
fn via_game_port() -> bool {
    !matches!(
        std::env::var("MC_PACK_VIA_GAME_PORT").as_deref(),
        Ok("0" | "false" | "off" | "no")
    )
}

static SETTINGS: OnceLock<Settings> = OnceLock::new();

/// 이 프로세스가 클라이언트에게 팩을 권하게 합니다. `url`이 없으면
/// `http://<클라이언트가 접속한 주소>:<게임 포트 또는 port>/…`.
pub fn enable(port: u16, url: Option<String>, forced: bool) {
    let _ = SETTINGS.set(Settings { port, url, forced });
}

pub fn enabled() -> bool {
    SETTINGS.get().is_some()
}

/// IPv6 주소는 대괄호로.
fn url_host(host: &str) -> String {
    if host.contains(':') && !host.starts_with('[') {
        format!("[{host}]")
    } else {
        host.to_owned()
    }
}

/// 클라이언트에게 보낼 것: (UUID, 주소, SHA-1, 강제 여부). `origin`은 클라이언트가 접속한 주소와 포트.
pub fn offer(origin: &(String, u16)) -> Option<(u128, String, String, bool)> {
    let s = SETTINGS.get()?;
    let p = pack();
    let port = if via_game_port() && origin.1 != 0 {
        origin.1
    } else {
        s.port
    };
    let url = match &s.url {
        Some(u) => u.clone(),
        None => format!(
            "http://{}:{port}/dangu-{}.zip",
            url_host(&origin.0),
            &p.sha1_hex[..12]
        ),
    };
    Some((p.uuid, url, p.sha1_hex.clone(), s.forced))
}

/// 팩을 내려 주는 아주 작은 HTTP 서버 (GET만, `.zip`이면 팩).
pub async fn serve(listener: tokio::net::TcpListener) -> std::io::Result<()> {
    loop {
        let (s, _) = listener.accept().await?;
        tokio::spawn(answer(s));
    }
}

/// 게임 포트로 온 연결이 HTTP 요청인가 (`GET `·`HEAD`로 시작). 마인크래프트 핸드셰이크는 첫
/// 바이트가 길이, 둘째가 패킷 번호 0이라 `GE`·`HE`와 겹치지 않습니다.
pub async fn looks_like_http(s: &tokio::net::TcpStream) -> bool {
    let mut b = [0u8; 2];
    for _ in 0..50 {
        match s.peek(&mut b).await {
            Ok(2) => return &b == b"GE" || &b == b"HE",
            Ok(1) if !matches!(b[0], b'G' | b'H') => return false,
            Ok(0) | Err(_) => return false,
            _ => tokio::time::sleep(std::time::Duration::from_millis(20)).await,
        }
    }
    false
}

/// HTTP 연결 하나에 답합니다.
pub async fn answer(mut s: tokio::net::TcpStream) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let mut buf = vec![0u8; 4096];
    let mut len = 0;
    while len < buf.len() {
        match tokio::time::timeout(std::time::Duration::from_secs(10), s.read(&mut buf[len..]))
            .await
        {
            Ok(Ok(0)) | Err(_) | Ok(Err(_)) => break,
            Ok(Ok(n)) => len += n,
        }
        if buf[..len].windows(4).any(|w| w == b"\r\n\r\n") {
            break;
        }
    }
    let reply = http_reply(&String::from_utf8_lossy(&buf[..len]));
    let _ = s.write_all(&reply).await;
    let _ = s.shutdown().await;
}

/// 요청 머리에 대한 HTTP 답 전체.
pub fn http_reply(req: &str) -> Vec<u8> {
    let path = req.split_whitespace().nth(1).unwrap_or("");
    let head_only = req.starts_with("HEAD ");
    let ok = (req.starts_with("GET ") || head_only) && path.ends_with(".zip");
    let current = pack();
    let (status, body, kind): (&str, &[u8], &str) = if ok {
        ("200 OK", &current.bytes, "application/zip")
    } else {
        ("404 Not Found", b"not found", "text/plain")
    };
    let mut out = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {kind}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    )
    .into_bytes();
    if !head_only {
        out.extend_from_slice(body);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn http_answers() {
        let r = String::from_utf8_lossy(&http_reply("GET /dangu-abc.zip HTTP/1.1\r\n\r\n"))
            .into_owned();
        assert!(r.starts_with("HTTP/1.1 200") && r.contains("application/zip"));
        let h = http_reply("HEAD /dangu-abc.zip HTTP/1.1\r\n\r\n");
        assert!(h.ends_with(b"\r\n\r\n"), "HEAD에는 본문 없음");
        let n = String::from_utf8_lossy(&http_reply("GET / HTTP/1.1\r\n\r\n")).into_owned();
        assert!(n.starts_with("HTTP/1.1 404"));
        assert_eq!(url_host("::1"), "[::1]");
        assert_eq!(url_host("play.example.com"), "play.example.com");
    }

    #[test]
    fn png_decodes_back() {
        let rgba = crate::plugins::lottery::art::LOTTERY_TICKET.rgba();
        let file = png(16, 16, &rgba);
        assert_eq!(&file[1..4], b"PNG");
        // IDAT를 풀면 행마다 필터 바이트 + 원래 픽셀
        let idat = file.windows(4).position(|w| w == b"IDAT").unwrap();
        let len = u32::from_be_bytes(file[idat - 4..idat].try_into().unwrap()) as usize;
        let raw = zlib::decompress(&file[idat + 4..idat + 4 + len], 1 << 20).unwrap();
        assert_eq!(raw.len(), 16 * (1 + 64));
        assert_eq!(&raw[1..65], &rgba[..64]);
    }

    #[test]
    fn pack_zip_lists_every_file_and_is_stable() {
        let p = pack();
        assert_eq!(&p.bytes[..4], &[0x50, 0x4b, 0x03, 0x04]);
        let text = String::from_utf8_lossy(&p.bytes);
        for name in [
            "pack.mcmeta",
            // 복권: 종이의 custom_model_data 10001 → 직접 만든 3D 모델
            "assets/minecraft/items/paper.json",
            "assets/dangu/models/item/lottery_ticket.json",
            "assets/dangu/textures/item/lottery_ticket.png",
            "assets/dangu/textures/item/lottery_ticket_back.png",
            // HUD: 줄 폰트, 바닐라 글자 폰트, 그림판, 셰이더, 숨긴 하트
            "assets/dangu/font/hud/t0.json",
            "assets/dangu/font/hud/mp.json",
            "assets/dangu/font/hud/text.json",
            "assets/dangu/textures/font/hud/bar.png",
            "assets/dangu/textures/font/hud/label.png",
            // 길드: 작물 성장 촉진제
            "assets/dangu/textures/item/crop_booster.png",
            "assets/minecraft/shaders/core/text.vsh",
            "assets/minecraft/textures/gui/sprites/hud/heart/full.png",
        ] {
            assert!(text.contains(name), "{name}");
        }
        let paper = crate::plugins::contents::current()
            .files
            .iter()
            .find(|(n, _)| n == "assets/minecraft/items/paper.json")
            .map(|(_, b)| String::from_utf8_lossy(b).into_owned())
            .unwrap();
        assert!(paper.contains("minecraft:custom_model_data"), "{paper}");
        assert!(paper.contains("dangu:item/lottery_ticket"));
        assert!(paper.contains("minecraft:item/paper"), "vanilla fallback");
        assert_eq!(p.sha1_hex.len(), 40);
        assert_eq!(
            zip(&[("a".into(), b"x".to_vec())]),
            zip(&[("a".into(), b"x".to_vec())])
        );
    }
}
