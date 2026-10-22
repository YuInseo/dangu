# mc-server

서버 프레임워크 없이 `tokio`만 쓰는 마인크래프트 Java Edition **26.3** (프로토콜 **777**) 서버.
암호(RSA·AES·SHA)와 압축(zlib)까지 직접 구현했고, 외부 구현은 Mojang 세션 서버와의
HTTPS(TLS) 하나뿐입니다 (`rustls`, 직접 짠 TLS는 보안 구멍이 되기 쉬워서).

```bash
cargo run --release                # 0.0.0.0:25565, 월드는 ./world, 정품 인증 켬
MC_ONLINE_MODE=false MC_GAMEMODE=creative cargo run --release
cargo test
```

| 환경 변수 | 기본값 | 뜻 (`server.properties` 대응) |
|---|---|---|
| `MC_BIND` | `0.0.0.0:25565` | 주소 |
| `MC_WORLD` | `world` | 월드 폴더 |
| `MC_SEED` | `20260915` | 시드 |
| `MC_ONLINE_MODE` | `true` | `online-mode` |
| `MC_COMPRESSION` | `256` | `network-compression-threshold` (음수 = 끔) |
| `MC_GAMEMODE` | `survival` | `gamemode` (처음 들어온 사람) |
| `MC_SPAWN_MOBS` | `true` | 몹 자연 스폰 |
| `MC_OPS` | (없음 = 모두) | 관리자 이름, 쉼표로 (`ops.json`) |
| `MC_OFFLINE_SKINS` | `false` | 오프라인 모드에서 같은 이름의 정품 스킨 빌리기 |

## 되는 것

**접속**
- Status(MOTD, 접속자 수), Login, Configuration(known pack으로 레지스트리 32개·태그), Play.
- 정품 인증: RSA-1024 키 교환 → AES-128-CFB8 암호화 → Mojang `hasJoined`. 스킨은 프로필
  속성 그대로 탭 목록에 실려 모두에게 보임.
- 압축: Set Compression 256, zlib 직접 구현 (청크 41KB → 1.4KB).
- 서명된 채팅: Mojang이 서명한 채팅 키 확인, 메시지 서명 확인, "읽은 메시지" 목록 재구성,
  서명째 전달 (확인 실패 시 서명 없는 채팅으로). 귓속말·`/me`도.

**월드**
- 오버월드·네더·엔드 지형, 청크 스트리밍(시야 최대 10), 블록 놓기·부수기와 저장.
- 놓을 때 방향(원목 축, 계단·화로·상자, 반 블록, 버튼, 물에 잠김, 문·침대 두 칸).
- 낮밤(Set Time 시계), `/time set`.

**서바이벌**
- 체력·배고픔·포만도, 낙하·용암·공허·굶주림 피해, 자연 회복, 먹기(바닐라 음식 데이터).
- 때리기(아이템 공격력), 넉백, 무적 시간, 사망 화면, 인벤토리 떨굼, 부활.
- 블록 전리품(바닐라 전리품 표), 곡괭이 등급, 떨어진 아이템(중력·줍기·Q로 버리기).
- 인벤토리 창, 2×2·작업대 3×3 조합(바닐라 조합법 1202개), 상자(같이 보기, 부수면 쏟음).
  아이템이 늘어나는 클릭은 거부하고 다시 보냄(복제 방지).

**몹**
- 소·돼지·양·닭(돌아다니고 맞으면 도망), 좀비·거미(밤에 나와 쫓아와 때림), 자연 스폰,
  죽으면 고기·가죽·실 등. 틱 루프는 초당 20틱.

**여럿이**
- 탭 목록, 서로 보이기(움직임·고개·든 아이템·팔 휘두르기), 입장·퇴장, 중복 접속 거부.

**저장** (`world/`)
- `blocks.dat` 바뀐 블록, `containers.dat` 상자, `players/<uuid>.dat` 위치·모드·체력·인벤토리.
  30초마다 + Ctrl+C로 끌 때 (플레이어는 10초마다와 나갈 때).

## 명령어

| 명령어 | 하는 일 | 관리자 |
|---|---|---|
| `/overworld` `/nether` `/end`, `/spawn` | 차원 이동, 스폰으로 | |
| `/msg <이름> <내용>` (`/tell`, `/w`), `/me` | 귓속말, 행동 | |
| `/list`, `/help` | 접속자, 도움말 | |
| `/tp <이름>` | 그 사람에게 | ✓ |
| `/gamemode <모드>` | 게임 모드 | ✓ |
| `/time set <day\|noon\|night\|midnight>` | 시간 | ✓ |
| `/give <아이템> [개수]` | 아이템 받기 | ✓ |
| `/summon <cow\|pig\|sheep\|chicken\|zombie\|spider>` | 몹 부르기 | ✓ |
| `/kill` | 죽기 | ✓ |

## 스피곳과 다른 점

스피곳(=바닐라 + 플러그인 API)과 같아지려면 아직 많이 남았습니다.

- **바닐라 지형·구조물**: 마을·요새·바이옴 없음, Anvil(.mca) 월드를 읽고 쓰지 않음.
- **살아 있는 월드**: 물·용암 흐름, 모래 떨어짐, 작물 자람, 레드스톤, 화로, 불, 폭발.
- **몹**: 길찾기(A*) 없음(벽은 한 축으로만 비켜 감), 스켈레톤·크리퍼·주민 등 없음, 경험치.
- **아이템**: 컴포넌트(이름·마법 부여·내구도) 무시, 갑옷 방어력, 도구 채굴 속도 검사.
- **채팅**: 서명된 명령어 인자(`/msg`의 서명)는 확인하지 않음. 채팅 신고용 체인 검증은 최소.
- **운영**: 밴·화이트리스트, 권한 단계, `server.properties` 파일 대신 환경 변수.
- **플러그인 API**: 스피곳 자바 플러그인을 돌리려면 JVM이 필요해서, Rust/WASM 플러그인이 현실적.

## 구조

```
src/protocol/            VarInt·기본 타입·NBT·zlib·패킷 코덱(압축·암호화)
src/crypto/              SHA-1/256, AES-CFB8, 큰 정수, RSA, Base64
src/auth.rs              Mojang 세션 서버 (HTTPS만 rustls)
src/chat.rs              서명된 채팅 (키 확인, 서명 대상, 읽은 메시지 창)
src/packets/             상태별 패킷 (ID와 형식)
src/connection.rs        Handshake → Status | Login
src/session/             Login → Configuration → Play, 명령어, 건축, 서바이벌, 창, 채팅
src/game/                엔티티 틱, 몹, 상자·플레이어 저장
src/world/               지형, 청크 인코딩, 바뀐 블록, 놓을 때의 상태
src/data/generated.rs    tools/gen_data.py가 만든 바닐라 데이터 (커밋됨)
```

## 프로토콜 자료의 출처

26.3 패킷 명세를 한곳에서 구할 수 없어 다음을 맞춰 썼습니다.

- 패킷 ID: ViaVersion `protocols/v26_2to26_3/packet/*Packets26_3` (열거 순서 = ID)
- 필드 형식: minecraft-data `pc/26.1` + ViaVersion 26.1→26.2→26.3 변환기의 차이
- 레지스트리·태그·조합법·전리품·아이템 성질: misode/mcmeta `26.3-registries`, `26.3-summary`
- 블록 상태·아이템·엔티티·창·명령어 인자 ID 순서: ViaVersion/Mappings `mapping-26.3.json`

데이터 다시 만들기:

```bash
python3 tools/gen_data.py \
  --mcmeta-registries <mcmeta 26.3-registries> \
  --mcmeta-summary <mcmeta 26.3-summary> \
  --via-mapping <Mappings>/mappings/mapping-26.3.json \
  --out src/data/generated.rs
```
