# mc-server

서버 프레임워크 없이 `tokio`만 쓰는 마인크래프트 Java Edition **26.3** (프로토콜 **777**) 서버.

```bash
cargo run --release                # 0.0.0.0:25565, 월드는 ./world
MC_BIND=127.0.0.1:25599 MC_SEED=42 MC_WORLD=/srv/mc cargo run --release
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
- **건축**: 블록 놓기·부수기가 모두에게 보이고(부서지는 소리·조각 포함) 저장됨.
  놓을 때 방향을 맞춤 — 원목 축, 계단·화로·상자 방향, 반 블록·계단 위아래, 버튼·레버
  붙는 면, 물에 잠김, 문·침대·큰 꽃은 두 칸. 문·침대를 부수면 나머지 반쪽도 함께.
- **인벤토리(크리에이티브)**: 크리에이티브 창에서 꺼낸 아이템, 핫바 선택, 가운데 클릭으로
  블록 집기. 손에 든 것과 휘두르는 팔이 다른 사람에게 보임.
- **저장**: 바뀐 블록만 `world/blocks.dat`에 30초마다, 그리고 Ctrl+C로 끌 때 저장.
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

## 스피곳과 같아지기까지

스피곳(=바닐라 서버 + 플러그인 API)의 기능은 바닐라 게임 로직 전체입니다. 한 번에 옮길
수 있는 양이 아니라서 단계로 나눕니다. ✅는 이 저장소에 있는 것.

1. **접속과 월드** ✅ — Status·Login·Configuration·Play, 세 차원, 청크 스트리밍
2. **여럿이 함께** ✅ — 탭 목록, 서로 보기, 채팅·귓속말, 기본 명령어
3. **건축과 저장** ✅ — 놓기·부수기, 방향, 크리에이티브 인벤토리, 저장
4. **서바이벌** — 체력·배고픔·낙하/용암 피해·죽음과 부활, 채굴 속도 검사, 드롭 아이템
   엔티티와 줍기, 서바이벌 인벤토리(컨테이너 클릭), 조합대·화로·상자
5. **월드가 살아 움직이기** — 틱 루프, 물·용암 흐름, 모래 떨어짐, 작물 자람, 블록 엔티티
   (상자·표지판), 레드스톤
6. **몹** — 엔티티 메타데이터, 스폰 규칙, 길찾기와 AI, 전투, 경험치
7. **바닐라 지형** — 노이즈 라우터·바이옴·구조물(마을, 요새 등), Anvil(.mca) 월드 읽기·쓰기
8. **운영** — 정품 인증(RSA·AES-CFB8·Mojang 세션 서버), 압축(zlib), 권한/OP, 밴·화이트리스트,
   `server.properties`
9. **플러그인 API** — 이벤트 버스와 명령어 등록. 스피곳 자바 플러그인을 그대로 돌리려면 JVM이
   필요하므로, Rust(동적 라이브러리)나 WASM 플러그인 쪽이 현실적입니다.

## 아직 안 되는 것 (3단계 기준)

정품 인증과 채팅 서명, 압축, 서바이벌 전부(위 4단계), 아이템 컴포넌트(이름·마법 부여),
스킨(오프라인이라 기본 스킨), 몹.

## 구조

```
src/protocol/            VarInt·기본 타입·NBT·프레임
src/packets/             상태별 패킷 (ID와 형식)
src/connection.rs        Handshake → Status | Login
src/session/             Login → Configuration → Play, 명령어
src/server.rs            접속자 명단, 월드 시간, 세션 사이 이벤트
src/world/terrain.rs     차원별 지형 생성
src/world/chunk.rs       Level Chunk with Light 인코딩
src/world/store.rs       바뀐 블록과 저장 파일
src/world/placement.rs   놓을 때의 블록 상태 정하기
src/data/blocks.rs       블록 상태 ID ↔ 속성
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
