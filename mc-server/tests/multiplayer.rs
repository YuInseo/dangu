//! 두 사람이 서로 보고, 말하고, 움직이고, 나가는 것.

mod common;

use common::{ids, join, recv_where, send, send_string, start_server};
use mc_server::protocol::types::read_uuid;
use mc_server::protocol::varint::read_varint;
use mc_server::uuid::offline_uuid;

const ADD_ENTITY: i32 = 0x01;
const DISGUISED_CHAT: i32 = 0x21;
const PLAYER_INFO_REMOVE: i32 = 0x46;
const PLAYER_INFO_UPDATE: i32 = 0x47;
const REMOVE_ENTITIES: i32 = 0x4E;
const SYSTEM_CHAT: i32 = 0x7C;
const TELEPORT_ENTITY: i32 = 0x80;

/// NBT 문자열 루트(8, u16 길이, 바이트)에서 글자를 꺼냅니다.
fn nbt_string(buf: &mut &[u8]) -> String {
    assert_eq!(buf[0], 8, "string tag");
    let len = u16::from_be_bytes([buf[1], buf[2]]) as usize;
    let s = String::from_utf8(buf[3..3 + len].to_vec()).unwrap();
    *buf = &buf[3 + len..];
    s
}

fn contains(haystack: &[u8], needle: &str) -> bool {
    haystack
        .windows(needle.len())
        .any(|w| w == needle.as_bytes())
}

#[tokio::test]
async fn two_players_see_and_talk_to_each_other() {
    let addr = start_server().await;
    let (mut alice, play) = join(addr, "Alice", 2).await;
    assert!(
        ids(&play).contains(&PLAYER_INFO_UPDATE),
        "Alice is listed herself"
    );
    assert!(ids(&play).contains(&0x73), "set time");

    let (mut bob, play) = join(addr, "Bob", 2).await;
    // Bob의 첫 탭 목록에는 자신과 Alice가 함께 있습니다.
    let info = play.iter().find(|p| p.id == PLAYER_INFO_UPDATE).unwrap();
    let mut b = &info.body[1..];
    assert_eq!(read_varint(&mut b).unwrap(), 2);

    // Bob이 들어오는 것을 Alice가 봅니다: 탭 목록 → 엔티티.
    let add = recv_where(&mut alice, |p| p.id == PLAYER_INFO_UPDATE).await;
    assert!(contains(&add.body, "Bob"));
    let spawn = recv_where(&mut alice, |p| p.id == ADD_ENTITY).await;
    let mut b = spawn.body.as_slice();
    read_varint(&mut b).unwrap();
    assert_eq!(read_uuid(&mut b).unwrap(), offline_uuid("Bob"));
    assert_eq!(
        read_varint(&mut b).unwrap(),
        mc_server::data::entity_type::PLAYER
    );

    // Bob은 이미 있던 Alice를 엔티티로 받습니다 (첫 청크 뒤에 옵니다).
    let spawn = recv_where(&mut bob, |p| p.id == ADD_ENTITY).await;
    let mut b = spawn.body.as_slice();
    read_varint(&mut b).unwrap();
    assert_eq!(read_uuid(&mut b).unwrap(), offline_uuid("Alice"));

    // 채팅: 모두에게, chat_type "chat"으로.
    send_string(&mut alice, 0x09, "안녕 §c밥").await;
    let chat = recv_where(&mut bob, |p| p.id == DISGUISED_CHAT).await;
    let mut b = chat.body.as_slice();
    assert_eq!(nbt_string(&mut b), "안녕 c밥", "서식 코드는 걸러집니다");
    let chat_type = read_varint(&mut b).unwrap();
    assert_eq!(
        chat_type,
        mc_server::data::registry_id("chat_type", "chat") + 1
    );
    assert_eq!(nbt_string(&mut b), "Alice");
    assert_eq!(b, [0], "대상 없음");

    // 귓속말: Bob만 받고, Alice에게는 보낸 기록이 남습니다.
    send_string(&mut alice, 0x07, "msg bob 비밀").await;
    let whisper = recv_where(&mut bob, |p| {
        p.id == DISGUISED_CHAT && contains(&p.body, "비밀")
    })
    .await;
    let mut b = whisper.body.as_slice();
    nbt_string(&mut b);
    assert_eq!(
        read_varint(&mut b).unwrap(),
        mc_server::data::registry_id("chat_type", "msg_command_incoming") + 1
    );
    let echo = recv_where(&mut alice, |p| {
        p.id == DISGUISED_CHAT && contains(&p.body, "비밀")
    })
    .await;
    let mut b = echo.body.as_slice();
    nbt_string(&mut b);
    assert_eq!(
        read_varint(&mut b).unwrap(),
        mc_server::data::registry_id("chat_type", "msg_command_outgoing") + 1
    );

    // 이동: Alice가 움직이면 Bob에게 Teleport Entity가 갑니다.
    let mut mv = Vec::new();
    for v in [3.5f64, 90.0, 4.5] {
        mv.extend_from_slice(&v.to_be_bytes());
    }
    mv.push(1);
    send(&mut alice, 0x1E, &mv).await;
    let tp = recv_where(&mut bob, |p| p.id == TELEPORT_ENTITY).await;
    let mut b = tp.body.as_slice();
    read_varint(&mut b).unwrap();
    assert_eq!(f64::from_be_bytes(b[..8].try_into().unwrap()), 3.5);

    // /list
    send_string(&mut bob, 0x07, "list").await;
    let list = recv_where(&mut bob, |p| {
        p.id == SYSTEM_CHAT && contains(&p.body, "접속자")
    })
    .await;
    assert!(contains(&list.body, "Alice, Bob"));

    // 차원이 다르면 서로 안 보입니다.
    send_string(&mut alice, 0x07, "nether").await;
    recv_where(&mut bob, |p| p.id == REMOVE_ENTITIES).await;

    // Alice가 나가면 탭 목록에서 빠집니다.
    drop(alice);
    let removed = recv_where(&mut bob, |p| p.id == PLAYER_INFO_REMOVE).await;
    let mut b = removed.body.as_slice();
    assert_eq!(read_varint(&mut b).unwrap(), 1);
    assert_eq!(read_uuid(&mut b).unwrap(), offline_uuid("Alice"));
    let bye = recv_where(&mut bob, |p| {
        p.id == SYSTEM_CHAT && contains(&p.body, "나갔습니다")
    })
    .await;
    assert!(contains(&bye.body, "Alice"));
}

#[tokio::test]
async fn same_name_twice_is_refused() {
    let addr = start_server().await;
    let (_first, _) = join(addr, "Twin", 2).await;

    // join()은 Play의 첫 청크 묶음을 기다리므로 여기서는 직접 끝까지 가서,
    // 두 번째 접속이 Play에 들어가자마자 Disconnect를 받는지 봅니다.
    let result = tokio::time::timeout(std::time::Duration::from_secs(10), async {
        let mut second = tokio::net::TcpStream::connect(addr).await.unwrap();
        use mc_server::protocol::types::{write_string, write_uuid};
        use mc_server::protocol::varint::write_varint;
        let mut hs = Vec::new();
        write_varint(&mut hs, 777);
        write_string(&mut hs, "localhost");
        hs.extend_from_slice(&25565u16.to_be_bytes());
        write_varint(&mut hs, 2);
        send(&mut second, 0x00, &hs).await;
        let mut hello = Vec::new();
        write_string(&mut hello, "Twin");
        write_uuid(&mut hello, 1);
        send(&mut second, 0x00, &hello).await;
        common::recv(&mut second).await;
        send(&mut second, 0x03, &[]).await;
        common::recv_until(&mut second, 0x0F).await;
        let mut packs = Vec::new();
        write_varint(&mut packs, 1);
        for v in ["minecraft", "core", "26.3"] {
            write_string(&mut packs, v);
        }
        send(&mut second, 0x07, &packs).await;
        common::recv_until(&mut second, 0x03).await;
        send(&mut second, 0x03, &[]).await;
        common::recv(&mut second).await
    })
    .await
    .unwrap();
    assert_eq!(result.id, 0x20, "Disconnect (play)");
    assert!(contains(&result.body, "이미 접속"));
}

#[tokio::test]
async fn gamemode_and_time_commands() {
    let addr = start_server().await;
    let (mut alice, _) = join(addr, "Alice", 2).await;
    let (mut bob, _) = join(addr, "Bob", 2).await;

    send_string(&mut alice, 0x07, "gamemode survival").await;
    let event = recv_where(&mut alice, |p| p.id == 0x27 && p.body[0] == 3).await;
    assert_eq!(
        f32::from_be_bytes(event.body[1..5].try_into().unwrap()),
        0.0
    );
    // Bob의 탭 목록에서도 Alice가 서바이벌로 바뀝니다 (동작 비트 0x04만).
    recv_where(&mut bob, |p| {
        p.id == PLAYER_INFO_UPDATE && p.body[0] == 0x04
    })
    .await;

    send_string(&mut bob, 0x07, "time set night").await;
    // 주기적인 시계 맞추기와 섞여 올 수 있으므로, 밤(13000)인 것을 기다립니다.
    let day_time = |body: &[u8]| {
        let mut b = &body[8..];
        assert_eq!(
            read_varint(&mut b).unwrap(),
            2,
            "overworld + the_end clocks"
        );
        read_varint(&mut b).unwrap();
        mc_server::protocol::varint::read_varlong(&mut b).unwrap()
    };
    recv_where(&mut alice, |p| {
        p.id == 0x73 && day_time(&p.body) % 24000 == 13000
    })
    .await;
}
