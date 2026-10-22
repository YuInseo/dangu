# mc-server

서버 프레임워크 없이 `tokio`만 쓰는 마인크래프트 Java Edition **26.3** (프로토콜 **777**) 서버.

```bash
cargo run                         # 0.0.0.0:25565
MC_BIND=127.0.0.1:25599 cargo run
cargo test
```

지금 되는 것: Handshake → Status(서버 목록 MOTD, 0/100) → Ping/Pong.
Login은 버전이 다르면 사유와 함께, 같으면 "아직 구현 안 됨"으로 끊습니다.

```
src/protocol/varint.rs   VarInt / VarLong 읽기·쓰기
src/protocol/types.rs    String, u16, i64 (빅엔디언)
src/protocol/frame.rs    길이 접두 프레임 (압축·암호화 전)
src/packets/             상태별 패킷
src/connection.rs        연결 하나의 상태 기계
```
