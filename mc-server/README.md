# mc-server

서버 프레임워크 없이 `tokio`만 쓰는 마인크래프트 Java Edition **26.3** (프로토콜 **777**) 서버.

```bash
cargo run --release                # 0.0.0.0:25565
MC_BIND=127.0.0.1:25599 MC_SEED=42 cargo run --release
cargo test
```

## 되는 것

- **Status**: 서버 목록 MOTD, 실시간 접속자 수 / 100, Ping/Pong.
- **Login** (오프라인 모드): 이름 → 바닐라와 같은 오프라인 UUID. 버전이 다르면 사유와 함께 끊음.
- **Configuration**: known pack(`minecraft:core` 26.3) 합의 → 동기화 레지스트리 32개,
  태그, 기능 플래그(`minecraft:vanilla`) 전송.
- **Play**: 크리에이티브로 접속, 주변 청크 스트리밍(시야 최대 8), 걸으면 새 청크를 받고
  먼 청크는 잊음, keep-alive.
- **여럿이 함께**: 탭 목록, 같은 차원의 다른 플레이어가 보이고 움직임·고개 돌림이 따라옴,
  입장·퇴장 알림, 같은 이름 중복 접속 거부.
- **채팅**: 바닐라 채팅 형식(`chat_type`)으로 전체 채팅, 귓속말, `/me`. 서식 코드(§)와
  제어 문자는 걸러냄.
- **세 차원 모두**: `/overworld` `/nether` `/end`로 오가고 `/spawn`으로 돌아감.
  - 오버월드: 언덕·바다·해변·참나무·풀, 아래로 돌·심층암·기반암
  - 네더: 네더랙 바닥과 천장, 용암 바다(y≤31), 영혼 모래, 발광석
  - 엔드: 원점의 엔드 돌 섬과 흑요석 기둥 10개

## 명령어

| 명령어 | 하는 일 |
|---|---|
| `/overworld` `/nether` `/end` | 차원 이동 |
| `/spawn` | 이 차원의 스폰으로 |
| `/msg <이름> <내용>` (`/tell`, `/w`) | 귓속말 |
| `/me <행동>` | `* 이름 행동` |
| `/tp <이름>` | 그 사람에게 (다른 차원이면 차원째) |
| `/gamemode <survival\|creative\|adventure\|spectator>` | 게임 모드 |
| `/time set <day\|noon\|night\|midnight>` | 모두의 시간 |
| `/list`, `/help` | 접속자 목록, 도움말 |

이름 인자는 클라이언트가 탭 목록으로 자동 완성합니다.

## 아직 안 되는 것

정품 인증(암호화)과 채팅 서명, 압축, 블록 부수기·놓기 저장, 스킨(오프라인이라 기본 스킨),
팔 휘두르기 등 애니메이션, 몹, 인벤토리, 체력·피해.

## 구조

```
src/protocol/            VarInt·기본 타입·NBT·프레임
src/packets/             상태별 패킷 (ID와 형식)
src/connection.rs        Handshake → Status | Login
src/session/             Login → Configuration → Play, 명령어
src/server.rs            접속자 명단, 월드 시간, 세션 사이 이벤트
src/world/terrain.rs     차원별 지형 생성
src/world/chunk.rs       Level Chunk with Light 인코딩
src/data/generated.rs    tools/gen_data.py가 만든 바닐라 데이터 (커밋됨)
```

## 프로토콜 자료의 출처

26.3 패킷 명세를 한곳에서 구할 수 없어 다음을 맞춰 썼습니다.

- 패킷 ID: ViaVersion `protocols/v26_2to26_3/packet/*Packets26_3` (열거 순서 = ID)
- 필드 형식: minecraft-data `pc/26.1` + ViaVersion 26.1→26.2→26.3 변환기의 차이
  (Login Finished의 세션 UUID, Login의 암호화 플래그, 게임 모드 VarInt화, 빛 마스크
  BitSet화, Accept Teleportation의 좌표 등)
- 레지스트리·태그: misode/mcmeta `26.3-registries`, `26.3-summary`
- 블록 상태·아이템 ID 순서: ViaVersion/Mappings `mapping-26.3.json`

데이터 다시 만들기:

```bash
python3 tools/gen_data.py \
  --mcmeta-registries <mcmeta 26.3-registries> \
  --mcmeta-summary <mcmeta 26.3-summary> \
  --via-mapping <Mappings>/mappings/mapping-26.3.json \
  --out src/data/generated.rs
```
