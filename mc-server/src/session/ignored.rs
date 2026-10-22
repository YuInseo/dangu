//! 받기만 하고 아무것도 하지 않는 Play 패킷과 그 이유. 목록에 없는 ID가 오면 한 번
//! 알려 줍니다 (ID 표가 어긋났을 때 바로 알 수 있도록).

use std::sync::Mutex;

/// (ID, 이름, 하지 않는 이유)
pub const IGNORED: &[(i32, &str, &str)] = &[
    (
        0x02,
        "BLOCK_ENTITY_TAG_QUERY",
        "F3+I 디버그 복사: 블록 엔티티 NBT를 보관하지 않음",
    ),
    (
        0x03,
        "BUNDLE_ITEM_SELECTED",
        "꾸러미 안 고르기: 꾸러미 내용물을 모델링하지 않음",
    ),
    (
        0x0B,
        "CHUNK_BATCH_RECEIVED",
        "청크 속도 조절 요청: 시야 거리 안 청크를 한 번에 보냄",
    ),
    (
        0x0D,
        "CLIENT_TICK_END",
        "틱 경계 표시: 서버 틱과 맞출 필요가 없음",
    ),
    (
        0x10,
        "CONFIGURATION_ACKNOWLEDGED",
        "재구성 응답: 인스턴스 이동 중에는 읽기 태스크가 따로 처리",
    ),
    (
        0x11,
        "CONTAINER_BUTTON_CLICK",
        "마법 부여·석재 절단기 버튼: 해당 창이 없음",
    ),
    (
        0x14,
        "CONTAINER_SLOT_STATE_CHANGED",
        "제작기 칸 잠금: 제작기 없음",
    ),
    (0x15, "COOKIE_RESPONSE", "쿠키를 요청하지 않음"),
    (0x16, "CUSTOM_PAYLOAD", "플러그인 채널 (minecraft:brand 등)"),
    (0x17, "DEBUG_SUBSCRIPTION_REQUEST", "개발용 디버그 렌더러"),
    (0x18, "EDIT_BOOK", "책과 깃펜 내용: 저장하지 않음"),
    (
        0x19,
        "ENTITY_TAG_QUERY",
        "F3+I 디버그 복사: 엔티티 NBT를 보관하지 않음",
    ),
    (0x1B, "JIGSAW_GENERATE", "직소 블록 (구조물 제작 도구)"),
    (0x22, "MOVE_VEHICLE", "탈것 없음"),
    (0x23, "PADDLE_BOAT", "보트 없음"),
    (
        0x27,
        "PLACE_RECIPE",
        "레시피 책 자동 채우기: 레시피 책을 보내지 않음",
    ),
    (
        0x2C,
        "PLAYER_LOADED",
        "월드 로딩 끝: 기다리지 않고 바로 보냄",
    ),
    (0x2D, "PONG", "서버 PING을 보내지 않음"),
    (0x2F, "RECIPE_BOOK_CHANGE_SETTINGS", "레시피 책 설정"),
    (0x30, "RECIPE_BOOK_SEEN_RECIPE", "레시피 책 표시"),
    (0x31, "RENAME_ITEM", "모루 없음"),
    (0x32, "RESOURCE_PACK", "리소스 팩을 보내지 않음"),
    (0x33, "SEEN_ADVANCEMENTS", "발전 과제 없음"),
    (0x35, "SET_BEACON", "신호기 없음"),
    (0x37, "SET_COMMAND_BLOCK", "명령 블록 없음"),
    (0x38, "SET_COMMAND_MINECART", "명령 블록 광산 수레 없음"),
    (
        0x3A,
        "SET_GAME_RULE",
        "게임 규칙 편집 화면 (규칙은 설정 파일로)",
    ),
    (0x3B, "SET_JIGSAW_BLOCK", "직소 블록"),
    (0x3C, "SET_STRUCTURE_BLOCK", "구조물 블록"),
    (0x3D, "SET_TEST_BLOCK", "테스트 블록"),
    (
        0x3E,
        "SIGN_UPDATE",
        "표지판 글: 블록 엔티티를 보관하지 않음",
    ),
    (0x41, "TEST_INSTANCE_BLOCK_ACTION", "게임 테스트 블록"),
    (
        0x44,
        "CUSTOM_CLICK_ACTION",
        "대화 상자(dialog) 버튼: 대화 상자를 보내지 않음",
    ),
];

static UNKNOWN_SEEN: Mutex<Vec<i32>> = Mutex::new(Vec::new());

pub fn note(id: i32) {
    if IGNORED.iter().any(|(i, ..)| *i == id) {
        return;
    }
    let mut seen = UNKNOWN_SEEN.lock().unwrap();
    if !seen.contains(&id) {
        seen.push(id);
        eprintln!("처리하지 않는 Play 패킷 0x{id:02X}");
    }
}
