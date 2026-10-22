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
| `MC_WORLD` | `world` | 기본 폴더 (인스턴스 목록·권한·인스턴스별 월드) |
| `MC_SEED` | `20260915` | 처음 만들 때의 시드 |
| `MC_INSTANCES` | `main` | 처음 켤 때 만들 인스턴스: `이름[:시드[:모드]]`, 쉼표로 |
| `MC_PREGEN` | `8` | 새 월드를 만들 때 스폰 둘레를 미리 만들 반지름(청크, 0 = 안 함) |
| `MC_ONLINE_MODE` | `true` | `online-mode` |
| `MC_COMPRESSION` | `256` | `network-compression-threshold` (음수 = 끔) |
| `MC_GAMEMODE` | `survival` | `gamemode` (처음 들어온 사람) |
| `MC_SPAWN_MOBS` | `true` | 몹 자연 스폰 |
| `MC_OPS` | (없음) | 처음 들어오면 OP 4가 될 이름, 쉼표로 (`ops.json`에 기록) |
| `MC_EVERYONE_OP` | `false` | 모두를 OP 4로 (혼자 시험할 때) |
| `MC_DIFFICULTY` | `normal` | `peaceful` `easy` `normal` `hard` |
| `MC_HARDCORE` | `false` | 하드코어 (죽으면 관전 모드) |
| `MC_OFFLINE_SKINS` | `false` | 오프라인 모드에서 같은 이름의 정품 스킨 빌리기 |

## 인스턴스 (한 포트에 여러 서버)

서버 하나에 **인스턴스**를 여러 개 둘 수 있습니다. 인스턴스마다 월드(지형 시드·바뀐 블록)·
몹·상자·플레이어 저장·접속자 목록·채팅·시간·난이도·기본 게임 모드가 따로이고, OP·권한·밴·
화이트리스트·정품 인증·사용자 정의 아이템은 함께 씁니다. BungeeCord·Velocity처럼 플레이어를
옮기지만 프록시 없이, 클라이언트를 Configuration 단계로 되돌렸다가(`Start Configuration`)
다른 인스턴스의 Play로 들여보냅니다.

```
world/
  instances.json      인스턴스 목록 (기본 인스턴스, 이름·폴더·시드·모드·난이도·하드코어)
  ops.json …          권한·밴 (네트워크 전체)
  custom_items.json
  lobby/              인스턴스마다: level.json, blocks.dat, containers.dat, players/
  survival/
```

- 처음 켤 때 `instances.json`이 없으면 `MC_INSTANCES`(없으면 `main` 하나)로 만들고, 각 폴더에
  `level.json`(이름·시드·만든 때·생성기 판·스폰)을 씁니다. 예전 단일 월드(`world/blocks.dat`)는
  그대로 첫 인스턴스가 됩니다.
- `/server [이름]` 목록·이동 (권한 `dangu.server.<이름>`으로 막을 수 있음),
  `/instance list|create|remove|default` 실행 중에 만들고 내리기 (OP 4).

## 담비 마스터 노드 (인스턴스마다 독립 프로세스)

`mc-server master`는 **마스터 노드**(프록시 + 관리자 CLI)로 켜지고, 컨텐츠 월드(마을·야생·RPG
…)마다 자기 실행 파일을 `child` 모드의 **독립된 OS 자식 프로세스**로 띄웁니다. 자식 하나 =
인스턴스 하나 = 오버월드·네더·엔드 세 차원.

```
플레이어 ──▶ 마스터 :25565 ──(핸드셰이크·Login Start만 읽고 라우팅)──▶ 127.0.0.1:25601 마을
              │  CLI (stdin)                                       ├──▶ 127.0.0.1:25602 야생
              └─ IPC: 유닉스 소켓 dambi/master.sock, [u32 길이][JSON] ┴──▶ 127.0.0.1:25603 rpg
```

```bash
DAMBI_HOME=dambi MC_BIND=0.0.0.0:25565 cargo run --release -- master
> instance start rpg --ram 4G        # = 인스턴스 시작 rpg --램 4G
> instance list                      # pid, 포트, 접속자, 램 사용/할당, cgroup 여부
> move Steve rpg                     # = 이동 Steve rpg
> money Steve give 1000 cash         # = 재화 Steve 지급 1000 캐시
> broadcast 10분 뒤 점검              # = 공지 … (모든 인스턴스)
> send rpg time set night            # 그 인스턴스 콘솔에서 실행
> stop                               # 모두 저장하고 끔
```

- **라우팅**: 프록시는 핸드셰이크와 Login Start(이름)만 읽고 그 뒤로는 바이트를 그대로 잇습니다
  (정품 인증·암호화·압축은 인스턴스가 직접). 진짜 IP는 핸드셰이크 주소 칸에
  `주소\0IP`로 실어 넘기고, 자식은 127.0.0.1에서만 들으므로 위조할 수 없습니다. 서버 목록
  핑은 마스터가 모든 인스턴스 접속자 합으로 답합니다.
- **인스턴스 이동**: 바닐라 **Transfer 패킷**. 자식이 `TransferRequest`를 보내면 마스터가
  "이 이름은 다음 접속 때 rpg로"라고 적어 두고, 자식이 클라이언트에게 Transfer(처음 접속한
  주소)를 보냅니다. 다시 들어오면 프록시가 rpg 프로세스로 잇습니다. LocalData(차원·좌표·체력·
  인벤토리)는 떠난 인스턴스의 플레이어 파일에 남고 새 인스턴스는 자기 것을 씁니다(Drop → Swap).
- **SharedData**는 마스터만 고칩니다: 돈·캐시·잠수포인트, 누적 플레이 시간(접속~퇴장, 5초마다
  정산), 생활 직업 레벨(필요 경험치 `3.0 × 레벨² + 300.0`, 전직 초기화 시 80% 환급). 마스터
  메모리에 캐시하고 바뀐 것만 5초마다 `dambi/shared/<uuid>.json`에 씁니다.
- **램 할당량**: cgroup v2를 쓸 수 있으면 `<cgroup>/dambi/<이름>/memory.max`로 커널이 막고,
  언제나 `/proc/<pid>/status` RSS를 2초마다 봐서 90%면 경고, 3번 연속 넘으면 저장·종료 후
  다시 켭니다. 비정상 종료(켠 지 30초 넘은 것)도 다시 켭니다.
- **전역 버스**: 자식의 `dambi::child::publish(채널, 내용)` → 마스터 → 모든 인스턴스의
  `subscribe(채널, …)`. `broadcast` 채널은 채팅 공지로 보입니다. 같은 JSON 메시지를 Redis 등
  다른 버스로 옮겨 실을 수 있게 형식을 단순하게 두었습니다.
- 게임 안 명령어: `/재화 [보내기 <이름> <양>]`, `/이동 [인스턴스]`. 마스터 없이 켠 서버에서도
  `/재화`는 그 서버 안의 저장소(`world/shared/`)로, `/이동`은 프로세스 안 인스턴스로 동작합니다.
- 설정: `dambi/dambi.json`(인스턴스 이름·포트·램·시드·모드·자동 시작). 처음 켜면 마을·야생·rpg.
  월드는 `dambi/instances/<이름>/`.
- 아직 없는 것: Wasm 플러그인 핫 리로딩(`wasmtime`), Redis 버스, 광부·강화 등 컨텐츠 로직은
  이 뼈대 위에 올릴 다음 단계입니다 (의존성을 `tokio`로 묶어 두었기 때문).

## 복권 (채널별, 매주 토요일 추첨)

- `/복권`(또는 스폰의 **복권 판매소 NPC** 우클릭) → 판매소 창 → **번호 고르기 창**(6줄 상자, 가운데
  7×4칸에 1~28번 종이). 번호 5개를 고르고(무작위 채우기·다시 고르기 버튼) 사면 돈이 빠지고 복권
  아이템이 들어옵니다. 하루(한국 시각)에 한 사람 2장까지.
- 복권은 한 장마다 **일련번호**가 있는 아이템입니다. 로어에 번호·아이템 번호·구매 날짜·추첨
  시각, 추첨 뒤엔 당첨 번호와 결과가 나옵니다. 그림은 서버가 직접 그린 16×16 도트(리소스 팩).
- 추첨: 토요일 21:00(한국 시각)에 번호 5개. 5개 1등, 4개 2등, 3개 3등, 2개 4등.
  1등 = (이번 회차 판매 장 수 × 가격) + 1등 기본 당첨금, 여럿이면 n분의 1. 1등은 서버 전체 공지.
- NPC에게 "당첨 복권 내기" → 가방의 복권을 모두 확인해 당첨금을 SharedData의 돈으로 줍니다.
- 설정·기록: 인스턴스 폴더의 `lottery.json` (가격, 하루 제한, 등수별 기본 당첨금, 추첨 요일·시각).
  `/복권 정보`, `/복권 추첨`(OP 4, 지금 추첨).

## 사용자 정의 NPC (`/npc`, `/엔피시`)

Citizens처럼 채널마다 NPC를 세웁니다 (`npcs.json`에 저장, 켤 때 다시 세움).

- 모양: `player`(마네킹 — 플레이어 모습, 탭 목록에 안 뜸, `/npc skin <이름>`으로 스킨) 또는
  몹(`villager`, `zombie` …). 머리 위 이름(`&a` 색 코드), 마네킹은 이름 아래 설명 한 줄.
- 움직이지 않고 맞지 않으며, 가까운 사람을 쳐다봅니다 (`/npc look`으로 끄기).
- 우클릭: 메시지(`/npc msg add`), 동작(`/npc action lottery` 복권 판매소, `server:<인스턴스>`
  다른 채널로), 명령어(`/npc cmd add [console] <명령어>`, `{player}` = 누른 사람).
- `/npc create <이름>`, `select [번호]`, `list`, `info`, `rename`, `type`, `skin`, `desc`,
  `tphere`, `remove` (한국어: 생성·선택·목록·정보·이름·타입·스킨·설명·여기로·삭제…). OP 2.
- 복권 판매소도 이 NPC입니다 (처음 켤 때 스폰 옆에 세움, 옮기거나 꾸밀 수 있음).

## 콘텐츠 (ItemsAdder식 리소스 팩 플러그인)

설정 파일과 그림만 넣으면 우리만의 아이템·블록·이모지가 생기고, 리소스 팩이 자동으로 만들어져
내려갑니다. 설정 모양은 ItemsAdder와 같습니다 (`info.namespace`, `items.<id>.display_name /
lore / resource{material, generate, textures | model_path} / graphics{texture | model, parent} /
enchant_glint / rarity / max_stack_size / attribute_modifiers.mainhand.attackDamage /
specific_properties.block.placed_model{type: REAL_NOTE}`, `font_images`, `categories`).

```
contents/                       MC_CONTENTS (기본 ./contents, 모든 인스턴스가 같이 씀)
  <팩>/configs/*.yml
  <팩>/resourcepack/assets/<ns>/textures/...   (모델·소리·폰트 등 무엇이든 그대로 팩에)
  .cache/ids.json               블록 상태·이모지 글자 배정 기억
```

- 아이템: `generate: true`면 모델(`item/generated`, 도구는 `item/handheld`)과 `items/<id>.json`을
  만들어 `item_model`로 연결. `model_path`면 직접 만든 모델.
- 블록(REAL_NOTE): 켜진 소리 블록 상태를 하나씩 빌려(바닐라 소리 블록은 그대로 보임) 진짜 블록으로
  놓이고, 부수면 그 아이템이 떨어집니다. `blockstates/note_block.json`을 만들어 줍니다.
- 이모지: `font_images` → 사용자 영역 글자(U+E000~)와 비트맵 폰트. 채팅의 `:heart:`가 그림으로,
  아이템 이름·설명의 `:heart:`도.
- `/ia` 분류별 목록 창(누르면 1개, Shift+클릭 한 묶음), `/iaget <ns:id> [개수]`, `/iaimage`,
  `/iareload`(=`/iazip`, OP 4): 다시 읽고 팩을 새로 만들어 접속한 사람들에게 바로 보냄. 담비
  마스터에선 CLI `iazip`(=`팩`) — 마스터가 내려 주는 팩과 모든 인스턴스가 함께 새로.
- 예시: `MC_CONTENTS=examples/contents cargo run --release` — 루비·루비 검·루비 블록·하트 이모지
  (그림도 직접 그린 도트).
- ItemsAdder의 가구·HUD·몹 모델·이벤트/행동(behaviours)·조합법·전리품·내구도는 아직 없습니다.

## 서버 리소스 팩

서버가 팩을 코드로 만들어(PNG·ZIP 직접 인코딩) `MC_PACK_PORT`(기본 25566)의 내장 HTTP로
내려 주고, Configuration 단계에서 클라이언트에게 권합니다. `MC_PACK_REQUIRED`(기본 켬)면
거절할 때 접속을 끊습니다. `MC_PACK_URL`로 다른 곳(CDN)에 올린 주소를 줄 수도 있습니다.
`cargo run --example resource_pack -- dangu.zip`으로 팩을 파일로 볼 수 있습니다.

## 되는 것

**접속**
- Status(MOTD, 접속자 수), Login, Configuration(known pack으로 레지스트리 32개·태그), Play.
- 정품 인증: RSA-1024 키 교환 → AES-128-CFB8 암호화 → Mojang `hasJoined`. 스킨은 프로필
  속성 그대로 탭 목록에 실려 모두에게 보임.
- 압축: Set Compression 256, zlib 직접 구현 (청크 41KB → 1.4KB).
- 서명된 채팅: Mojang이 서명한 채팅 키 확인, 메시지 서명 확인, "읽은 메시지" 목록 재구성,
  서명째 전달 (확인 실패 시 서명 없는 채팅으로). 귓속말·`/me`도.

**월드 생성** (`src/world/`)
- **바이옴 65종**: 기후 노이즈(온도·습도·대륙성·침식·기묘함)로 높이와 바이옴을 함께 정합니다 —
  바다·깊은 바다·해안·강·평지·숲·타이가·정글·사바나·사막·황무지(색 띠 테라코타)·늪·버섯 섬·
  산(초원·벚나무 숲·숲 언덕·눈 비탈·봉우리)·얼음 가시. 26.3의 **얼룩진 숲(dappled forest)**은
  빨강·주황·노랑 **포플러** 나무와 낙엽.
- **동굴**: 3차원 노이즈의 넓은 굴과 국수 굴, -55 아래 용암, 동굴 바이옴(무성한 동굴, 점적석 동굴,
  깊은 어둠, 26.3의 **유황 동굴** — 유황·짙은 유황·유황 가시).
- 광석 13종(심층암 변종 포함), 나무 16종(참나무·자작·가문비·소나무·거대 가문비·정글·아카시아·
  짙은 참나무·창백한 참나무·벚나무·맹그로브·포플러·거대 버섯 …), 풀·꽃·선인장(+선인장 꽃)·
  사탕수수·대나무·수련·해초·켈프·눈·얼음. 나무와 구조물은 청크 경계를 넘어도 이어집니다.
- 네더 바이옴 5종(진홍·뒤틀린 숲의 거대 균류, 영혼 모래 골짜기, 현무암 삼각주, 황무지)과
  석영·금·고대 잔해. 엔드 가운데 섬 + 1000블록 밖 바깥 섬(엔드 고지대, 후렴 식물).
- **구조물 34종**을 표로 관리합니다: 위치는 바닐라 구조물 묶음(간격·분리·소금, Java `Random`)을
  그대로 써서 바닐라와 같은 후보 청크, 요새는 동심원 배치, 생기는 바이옴은 바닐라
  `has_structure` 태그. 마을 5종(우물·길·집·농장·대장간·도서관), 사막·정글 사원, 이글루(지하실),
  늪 오두막, 약탈자 전초 기지, 삼림 대저택, 무너진 차원문 7종, 난파선, 바다 유적, 바다 신전,
  묻힌 보물, 옛길 유적, 폐광, 요새(엔드 차원문 방), 시련의 회당, 고대 도시, 네더 요새, 보루 잔해,
  네더 화석, 엔드 도시, 그리고 26.3의 **버려진 야영지**(바이옴 18종).
- 구조물 상자는 처음 열 때 **바닐라 전리품 표 58개**로 채워집니다. 상자·생성기 등은 블록
  엔티티로 보내 클라이언트가 그립니다. `/locate structure|biome <이름>`.
- 팬텀은 만들지 않습니다 (팬텀 없는 서버).
- 개발용: `cargo run --release --example mapview -- <시드> <x> <z> <반지름> map.png [biome]`로
  내려다본 지도, `--example genbench`로 생성 속도(오버월드 약 5ms/청크).

**살아 있는 월드** (`src/game/mechanics/`)
- 예약 틱: 블록이 바뀌면 반응하는 이웃에만 틱을 걸어 처리 — 물·용암 흐름(원천·흐름 세기 1~7·
  떨어짐, 원천 둘 사이의 새 원천, 용암+물 → 흑요석/조약돌), 모래·자갈 떨어짐, 받침을 잃은
  풀·꽃·작물·횃불이 부서짐.
- 무작위 틱(섹션마다 매 틱 3칸): 작물·줄기 열매·베리·선인장·사탕수수·대나무·켈프 자람,
  묘목 → 나무, 잔디 퍼짐, 농지 젖음·마름.
- 화로·용광로·훈연기: 바닐라 굽기 조리법(화로 73·용광로 25·훈연기 9)과 연료 시간, 불·진행
  화살표 표시, 블록 `lit`.
- 레드스톤: 레버·버튼·압력판·레드스톤 블록·횃불(인버터)·중계기(지연), 가루(15에서 한 칸마다
  1씩, 계단 연결, 모양), 램프·문·다락문·울타리 문·TNT가 받음. 손으로 문·레버 조작.
- TNT(부싯돌·레드스톤으로 점화, 4초 뒤 폭발), 폭발(블록·드롭·피해·밀어냄·연쇄, 흑요석 버팀).
- 네더 차원문: 흑요석 틀에 부싯돌 → 차원문, 4초 서 있으면 오버월드↔네더(8:1), 도착지에
  문이 없으면 만듦.

**월드**
- 청크 스트리밍(시야 최대 10), 블록 놓기·부수기와 저장.
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

**게임 모드**
- 서바이벌·크리에이티브·모험·관전. F3+F4 전환(권한 필요), 비행, 크리에이티브 즉시 부수기·
  아이템 꺼내기·던지기·블록 집기, 서바이벌 가운데 클릭(가진 것에서 찾아 쥐기), F로 손 바꾸기.
- 관전: 엔티티 좌클릭으로 빙의(Set Camera), 웅크려서 풀기, 관전 메뉴로 다른 사람에게 순간 이동.
- 난이도(설정 화면 버튼·자물쇠 포함): 평화로움은 적대 몹 없음·자동 회복, 몹 공격력 보정,
  굶주림 하한(쉬움 10, 보통 1, 어려움 0). 하드코어 하트, 죽으면 관전.

**사용자 정의 아이템** (리소스 팩 없이, `world/custom_items.json`)
- 아이템 컴포넌트로 이름·색·설명·반짝임·희귀도·최대 개수와 **바닐라 모델 빌려 쓰기**
  (`item_model`), 서버 쪽 능력(번개, 순간 이동, 도약, 회복)과 재사용 대기 표시.
- 새 텍스처는 리소스 팩 없이는 불가능합니다 (클라이언트에 그림이 없음).

**OP와 권한** (Spigot과 같은 파일: `ops.json`, `permissions.json`, `banned-players.json`,
`banned-ips.json`, `whitelist.json`)
- OP 0~4단계 (클라이언트에 Entity Event 24~28로 알려 F3+F4 등이 켜짐).
- 권한 노드 `dangu.command.<명령어>`, `*` 와일드카드, `-노드`로 빼기, 그룹과 상속, `default` 그룹.
  규칙이 없으면 명령어마다 정한 OP 단계를 따릅니다.
- 밴·IP 밴·화이트리스트는 로그인 때 확인. 명령어 트리는 권한 있는 것만 보냄(나머지는 빨간 글씨).
- 표준 입력 콘솔 (모든 권한).

**HUD·기타**
- 제목·부제목, 액션 바, 탭 목록 머리·꼬리, 서버 정보, 핑(퐁), 서버 자동 완성(`/pardon`, `/perm`),
  Play 중 설정 변경(시야 거리·겉옷 레이어·주 손), 웅크리기·달리기가 다른 사람에게 보임.
- 처리하지 않는 패킷은 `src/session/ignored.rs`에 이유와 함께 적어 둠.

**여럿이**
- 탭 목록, 서로 보이기(움직임·고개·든 아이템·팔 휘두르기), 입장·퇴장, 중복 접속 거부.

**저장** — 바닐라 **Anvil 형식** (`world/<인스턴스>/`)
- `region/r.X.Z.mca`(네더 `DIM-1/region`, 엔드 `DIM1/region`): 청크마다 섹션 팔레트(블록 이름 +
  속성)·바이옴 팔레트·블록 엔티티(상자 내용, 안 연 구조물 상자의 `LootTable`, 생성기)·높이 맵,
  DataVersion 5023(26.3). 만든 청크와 바뀐 청크를 저장하고, 메모리에는 최근 청크만 둡니다
  (한 가지 블록뿐인 섹션은 값 하나로 압축).
- **바닐라 월드를 읽습니다**: 다른 서버에서 만든 26.x 월드 폴더를 인스턴스 폴더로 두면 그대로
  열립니다 (모르는 블록은 공기로, LZ4 압축 청크는 아직 못 읽음, 바닐라의 엔티티·플레이어
  파일은 가져오지 않음).
- `level.dat`(gzip NBT: 이름·시드·스폰·시간·난이도)와 `level.json`. **처음 켤 때** 새 월드면 둘을
  만들고 스폰 둘레를 여러 스레드로 미리 만들어 저장합니다 (13×13 청크 약 0.6초).
- 예전 형식 `blocks.dat`은 처음 열 때 영역 파일로 옮기고 `blocks.dat.old`로 남깁니다.
- `containers.dat` 상자, `players/<uuid>.dat` 위치·모드·체력·인벤토리. 30초마다 + 끌 때.

## 명령어

OP 단계는 권한 규칙이 없을 때의 기본값입니다 (`/perm`으로 사람·그룹마다 바꿀 수 있음).

| 명령어 | 하는 일 | OP |
|---|---|---|
| `/overworld` `/nether` `/end`, `/spawn` | 차원 이동, 스폰으로 | 0 |
| `/msg <이름> <내용>` (`/tell`, `/w`), `/me` | 귓속말, 행동 | 0 |
| `/list`, `/help` | 접속자, 도움말 | 0 |
| `/tp <이름>`, `/gamemode <모드> [이름]`, `/time set …`, `/difficulty …` | | 2 |
| `/give <아이템> [개수]`, `/item list\|reload\|<id>`, `/summon …`, `/kill` | | 2 |
| `/say`, `/title <이름\|@a> <제목>[\|부제목]` | | 2 |
| `/kick`, `/ban`, `/pardon`, `/ban-ip`, `/pardon-ip`, `/whitelist …` | | 3 |
| `/op <이름> [단계]`, `/deop`, `/perm …` | | 3 |
| `/server [이름]` | 인스턴스 목록·이동 | 0 |
| `/재화 [보내기 <이름> <양>]` (`/money`) | 돈·캐시·잠수포인트·플레이 시간·직업, 송금 | 0 |
| `/이동 [인스턴스]` (`/move`) | 다른 인스턴스 프로세스로 (마스터 뒤에서) | 0 |
| `/instance list\|create <이름> [시드] [모드] [난이도] [hardcore]\|remove\|default` | | 4 |
| `/locate structure\|biome <이름>` | 가장 가까운 구조물·바이옴 | 2 |
| `/save-all`, `/stop` | | 4 |

## 스피곳과 다른 점

스피곳(=바닐라 + 플러그인 API)과 같아지려면 아직 많이 남았습니다.

- **바닐라 지형 그대로는 아님**: 같은 시드라도 바닐라와 지형이 다릅니다(밀도 함수·직소 조각을
  옮기지 않음). 구조물 후보 위치만 바닐라와 같습니다. 
- **살아 있는 월드**: 피스톤·비교기·관측기, 불 번짐, 빛 계산(블록빛), 날씨.
- **몹**: A* 길찾기, 좀비·허스크·스켈레톤·스트레이·위더 스켈레톤·크리퍼·거미·동굴 거미·좀벌레·
  엔더맨·좀비 피글린·블레이즈·주민이 있고, 구조물 생성기에서 나오고, 마을엔 주민이 삽니다.
  **팬텀은 없습니다.** 우클릭: 주민 거래 창(바닐라 26.3 거래표, 낸 만큼 빠졌는지 서버가 확인,
  거래 경험치로 레벨이 오르면 새 거래), 양동이로 소 젖 짜기, 가위로 양털 깎기(풀을 뜯으면 다시
  자람). 아직 없는 것: 경험치 구슬, 물속 AI, 물약 효과, 번식, 가위 내구도.
- **아이템**: 마법 부여·내구도, 갑옷 방어력, 도구 채굴 속도 검사.
- **채팅**: 서명된 명령어 인자(`/msg`의 서명)는 확인하지 않음. 채팅 신고용 체인 검증은 최소.
- **운영**: `server.properties` 파일 대신 환경 변수.
- **플러그인 API**: 스피곳 자바 플러그인을 돌리려면 JVM이 필요해서, Rust/WASM 플러그인이 현실적.

## 구조 (코드 지도)

아래층은 위층을 모릅니다. 새 기능은 대부분 맨 위 `plugins/`에 폴더 하나로 들어갑니다.

```
 ┌ plugins/   이 서버만의 기능 — 기능 하나 = 폴더 하나
 │   contents/   ItemsAdder 호환: source(폴더 구조 5가지) → load → item_config, cache, builtin, catalog(/ia 창)
 │   lottery/    복권: mod(규칙·추첨·저장) · gui(번호 고르기 창·NPC 제출) · art(도트·3D 모델·설정)
 │   npc/        NPC: mod(저장·틱·스킨) · session(/npc 명령·우클릭)
 ├ session/   접속 하나: login → configuration → play
 │   play/       mod(세션 상태·루프) · packets(받은 패킷 처리) · events(다른 세션 소식) · view(청크·순간이동·엔티티 표시)
 │   commands · windows(창·클릭) · interact(우클릭) · survival · merchant(거래) · modes · hud · signed_chat
 ├ game/      틱으로 돌아가는 세계: entities, mobs/(mod·ai·movement), path(A*), projectiles,
 │            spawning, villagers, custom_items, containers, playerdata, mechanics/(물·작물·화로·레드스톤…)
 ├ world/     지형·바이옴·구조물(structure/)·청크 인코딩·저장(anvil, store)
 ├ server.rs / network.rs   인스턴스 하나의 공유 상태 · 인스턴스 여럿과 이동
 ├ dambi/     마스터 노드: master · proxy · cli · process(램) · ipc · shared · child
 ├ admin.rs / perms.rs     관리 명령어 · 권한·밴·화이트리스트
 ├ resource_pack.rs        팩 굽기(PNG·ZIP) + 내장 HTTP 서버
 ├ packets/   상태별 패킷 형식 (ID와 encode/decode)
 ├ protocol/  VarInt·NBT·zlib·프레임(압축·암호화)    crypto/  SHA·AES·RSA      auth.rs  Mojang 인증
 └ data/      바닐라 데이터 (tools/*.py가 만든 generated.rs, trades.rs, collision.rs, item_definitions.json.gz)
   json.rs · yaml.rs · text.rs · uuid.rs   작은 도구들
```

어디를 고치면 되나:

| 하고 싶은 것 | 파일 |
|---|---|
| 새 명령어 | `session/commands.rs` (권한 표는 `admin.rs`) |
| 받은 패킷 처리 바꾸기 | `session/play/packets.rs` |
| 몹 행동 | `game/mobs/ai.rs`, 걷기는 `game/mobs/movement.rs` |
| 새 서버 기능 | `plugins/<이름>/` 만들고 `plugins/mod.rs` 표에 한 줄 |
| ItemsAdder 설정 키 하나 더 읽기 | `plugins/contents/item_config.rs` |
| 리소스 팩에 파일 넣기 | 플러그인의 콘텐츠 팩 (`plugins/lottery/art.rs` 참고) |

도구: `tools/gen_data.py`(레지스트리·블록·아이템) · `gen_worldgen.py`(구조물·바이옴 태그·전리품) ·
`gen_trades.py`(주민 거래) · `gen_collision.py`(충돌 없는 블록) · `gen_item_definitions.py`(바닐라 아이템 정의).

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
