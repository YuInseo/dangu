//! 퀵메뉴: Shift+F로 열기 → 내 정보 → 생활 스탯 → 뒤로, 쓰레기통에 넣고 닫으면 사라짐.

mod common;

use std::sync::Arc;

use common::*;
use mc_server::Server;
use mc_server::data::items::by_name;
use mc_server::protocol::varint::{read_varint, write_varint};

const SYSTEM_CHAT: i32 = 0x7C;
const OPEN_SCREEN: i32 = 0x3C;
const SET_CONTENT: i32 = 0x12;

fn text_of(p: &mc_server::protocol::frame::RawPacket) -> String {
    String::from_utf8_lossy(&p.body).into_owned()
}

/// 열린 창 번호와 제목 (바이트 그대로).
async fn opened(s: &mut Client) -> (i32, String) {
    let p = recv_where(s, |p| p.id == OPEN_SCREEN).await;
    (read_varint(&mut p.body.as_slice()).unwrap(), text_of(&p))
}

fn plain(out: &mut Vec<u8>, item: Option<(i32, i32)>) {
    match item {
        None => out.push(0),
        Some((id, n)) => {
            out.push(1);
            write_varint(out, id);
            write_varint(out, n);
            write_varint(out, 0);
            write_varint(out, 0);
        }
    }
}

#[allow(clippy::type_complexity)]
fn click(
    window: i32,
    slot: i16,
    changed: &[(i16, Option<(i32, i32)>)],
    carried: Option<(i32, i32)>,
) -> Vec<u8> {
    let mut c = Vec::new();
    write_varint(&mut c, window);
    write_varint(&mut c, 0);
    c.extend_from_slice(&slot.to_be_bytes());
    c.push(0);
    write_varint(&mut c, 0);
    write_varint(&mut c, changed.len() as i32);
    for &(s, item) in changed {
        c.extend_from_slice(&s.to_be_bytes());
        plain(&mut c, item);
    }
    plain(&mut c, carried);
    c
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn shift_f_menu_and_trash() {
    let server = Arc::new(Server::new(test_info(), SEED));
    let addr = start_server_with(Arc::clone(&server)).await;
    let (mut s, _) = join(addr, "Menu", 2).await;

    // 웅크리고 (Player Input 0x20) F (Player Action 7 = 손 바꾸기)
    send(&mut s, 0x2B, &[0x20]).await;
    let mut action = Vec::new();
    write_varint(&mut action, 7);
    action.extend_from_slice(&0i64.to_be_bytes());
    action.push(0);
    write_varint(&mut action, 0);
    send(&mut s, 0x29, &action).await;
    let (w, title) = opened(&mut s).await;
    assert!(title.contains("퀵메뉴"), "{title}");

    // 내 정보 → 생활 스탯 → 뒤로(내 정보)
    send(&mut s, 0x12, &click(w, 46, &[], None)).await;
    let (w, title) = opened(&mut s).await;
    assert!(title.contains("내 정보"));
    send(&mut s, 0x12, &click(w, 12, &[], None)).await;
    let (w, title) = opened(&mut s).await;
    assert!(title.contains("생활 스탯"));
    send(&mut s, 0x12, &click(w, 18, &[], None)).await;
    let (_, title) = opened(&mut s).await;
    assert!(title.contains("내 정보"));

    // 쓰레기통: 흙 5개를 넣고 닫으면 사라짐
    let dirt = by_name("dirt").unwrap();
    send_string(&mut s, 0x07, "give dirt 5").await;
    recv_where(&mut s, |p| {
        p.id == SYSTEM_CHAT && text_of(p).contains("받았습니다")
    })
    .await;
    send_string(&mut s, 0x07, "쓰레기통").await;
    let (w, title) = opened(&mut s).await;
    assert!(title.contains("쓰레기통"));
    // 27칸 창: 핫바 0 = 54
    send(&mut s, 0x12, &click(w, 54, &[(54, None)], Some((dirt, 5)))).await;
    send(&mut s, 0x12, &click(w, 0, &[(0, Some((dirt, 5)))], None)).await;
    let mut close = Vec::new();
    write_varint(&mut close, w);
    send(&mut s, 0x13, &close).await;
    // 닫은 뒤 다시 보낸 인벤토리에 흙이 없음
    let p = recv_where(&mut s, |p| {
        p.id == SET_CONTENT && p.body.first() == Some(&0)
    })
    .await;
    let mut needle = Vec::new();
    write_varint(&mut needle, dirt);
    write_varint(&mut needle, 5);
    assert!(
        !p.body
            .windows(needle.len() + 1)
            .any(|w| w[0] == 1 && w[1..] == needle[..]),
        "흙이 돌아옴"
    );
}
