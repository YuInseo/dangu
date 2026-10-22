//! 서버 리소스 팩: Configuration에서 Add Resource Pack을 받고, 그 주소에서 실제로 내려받은
//! 파일의 SHA-1이 알려 준 것과 같은지. 적용하면 Play로, 강제인데 거절하면 접속 끊김.

mod common;

use std::sync::Arc;

use common::*;
use mc_server::Server;
use mc_server::protocol::types::{read_string, read_uuid, write_string, write_uuid};
use mc_server::protocol::varint::write_varint;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// Client Information과 known pack까지 보내고, 팩 제안(0x09)을 받습니다.
async fn until_pack_offer(s: &mut Client) -> (u128, String, String, bool) {
    let mut info = Vec::new();
    write_string(&mut info, "ko_kr");
    info.push(2);
    info.extend_from_slice(&[0, 1, 0x7f, 1, 0, 1, 0]);
    send(s, 0x00, &info).await;
    recv_until(s, 0x0F).await;
    let mut packs = Vec::new();
    write_varint(&mut packs, 1);
    for v in ["minecraft", "core", "26.3"] {
        write_string(&mut packs, v);
    }
    send(s, 0x07, &packs).await;
    let (offer, _) = recv_until(s, 0x09).await;
    let mut b = offer.body.as_slice();
    let uuid = read_uuid(&mut b).unwrap();
    let url = read_string(&mut b, 32767).unwrap();
    let hash = read_string(&mut b, 40).unwrap();
    let forced = b[0] == 1;
    (uuid, url, hash, forced)
}

async fn respond(s: &mut Client, uuid: u128, result: i32) {
    let mut b = Vec::new();
    write_uuid(&mut b, uuid);
    write_varint(&mut b, result);
    send(s, 0x06, &b).await;
}

/// 아주 단순한 HTTP GET.
async fn http_get(url: &str) -> Vec<u8> {
    let rest = url.strip_prefix("http://").unwrap();
    let (host, path) = rest.split_once('/').unwrap();
    let mut c = tokio::net::TcpStream::connect(host).await.unwrap();
    c.write_all(format!("GET /{path} HTTP/1.1\r\nHost: {host}\r\n\r\n").as_bytes())
        .await
        .unwrap();
    let mut all = Vec::new();
    c.read_to_end(&mut all).await.unwrap();
    let split = all.windows(4).position(|w| w == b"\r\n\r\n").unwrap();
    assert!(
        all.starts_with(b"HTTP/1.1 200"),
        "{}",
        String::from_utf8_lossy(&all[..split])
    );
    all[split + 4..].to_vec()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn pack_is_offered_downloadable_and_required() {
    let http = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = http.local_addr().unwrap().port();
    tokio::spawn(mc_server::resource_pack::serve(http));
    mc_server::resource_pack::enable(port, None, true);

    let server = Arc::new(Server::new(test_info(), SEED));
    let addr = start_server_with(Arc::clone(&server)).await;

    // 받아서 적용 → Play로
    let mut s = connect(addr).await;
    login(&mut s, "Packer").await;
    let (uuid, url, hash, forced) = until_pack_offer(&mut s).await;
    assert!(forced);
    assert!(
        url.starts_with(&format!("http://localhost:{port}/")),
        "{url}"
    );
    let zip = http_get(&url).await;
    let sha: String = mc_server::crypto::sha1::sha1(&zip)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    assert_eq!(sha, hash);
    assert_eq!(&zip[..2], b"PK");
    for r in [3, 4, 0] {
        respond(&mut s, uuid, r).await;
    }
    recv_until(&mut s, 0x03).await; // Finish Configuration
    send(&mut s, 0x03, &[]).await;
    recv_until(&mut s, 0x0B).await; // Play의 첫 청크 묶음

    // 거절 → 강제이므로 끊김
    let mut d = connect(addr).await;
    login(&mut d, "Decliner").await;
    let (uuid, ..) = until_pack_offer(&mut d).await;
    respond(&mut d, uuid, 1).await;
    let (bye, _) = recv_until(&mut d, 0x02).await;
    assert!(String::from_utf8_lossy(&bye.body).contains("리소스 팩"));
}
