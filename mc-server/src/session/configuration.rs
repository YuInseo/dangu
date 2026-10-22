//! Configuration: 레지스트리·태그 보내기, 리소스 팩, Play로 넘기기.
//!
//! 다른 클라이언트(Lunar·Feather·Badlion·모드 클라이언트)도 받아들이도록 너그럽게:
//! - 우리가 쓰지 않는 패킷(쿠키 응답, 플러그인 채널, 대화 상자 버튼, 행동 강령 동의, …)은 건너뜀
//! - 기다리는 동안 10초마다 keep-alive를 보냄 (클라이언트는 30초 동안 아무것도 못 받으면 끊음)
//! - 리소스 팩 받기·적용에 실패하면 내보내지 않고 팩 없이 들어감 (끊는 것은 "거절"뿐)

use std::time::{Duration, Instant};

use tokio::io::{AsyncRead, AsyncWrite};

use super::ClientSettings;
use crate::packets::configuration as cfg;
use crate::protocol::conn::Conn;
use crate::protocol::frame::{FrameError, RawPacket};
use crate::text;

/// 기본 시야 거리. Client Information이 오기 전이거나 안 오면 이것을 씁니다.
pub(crate) const DEFAULT_VIEW_DISTANCE: u8 = 8;

/// 이만큼 아무것도 오지 않으면 keep-alive를 보냅니다.
const KEEP_ALIVE_EVERY: Duration = Duration::from_secs(10);
/// 보통 응답(known packs, 설정 끝)을 기다리는 시간.
const REPLY_LIMIT: Duration = Duration::from_secs(60);
/// 리소스 팩을 받아 적용하기까지 (느린 회선·모드가 많은 클라이언트).
const PACK_LIMIT: Duration = Duration::from_secs(300);

/// 리소스 팩 응답 (`Resource Pack Response`의 결과).
mod pack_status {
    pub const LOADED: i32 = 0;
    pub const DECLINED: i32 = 1;
    pub const ACCEPTED: i32 = 3;
    pub const DOWNLOADED: i32 = 4;

    pub fn name(s: i32) -> &'static str {
        match s {
            2 => "받기 실패",
            5 => "주소가 잘못됨",
            6 => "적용 실패",
            7 => "버려짐",
            _ => "알 수 없는 응답",
        }
    }
}

/// 레지스트리·태그를 보내고 Play로 넘길 준비를 합니다. 클라이언트가 우리 known pack을
/// 갖고 있지 않으면 사유를 보내고 `None`. 인스턴스를 옮기며 다시 올 때는 클라이언트가
/// Client Information을 다시 보내지 않을 수 있어, 지난 설정(`settings`)을 이어받습니다.
pub async fn run<S>(
    stream: &mut Conn<S>,
    mut settings: ClientSettings,
) -> Result<Option<ClientSettings>, FrameError>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    stream
        .write_packet(cfg::CUSTOM_PAYLOAD_ID, &cfg::encode_brand("dangu"))
        .await?;
    stream
        .write_packet(
            cfg::UPDATE_ENABLED_FEATURES_ID,
            &cfg::encode_enabled_features(),
        )
        .await?;
    stream
        .write_packet(
            cfg::SELECT_KNOWN_PACKS_ID,
            &cfg::encode_known_packs(&[cfg::core_pack()]),
        )
        .await?;

    let packet = wait(
        stream,
        &mut settings,
        REPLY_LIMIT,
        &[cfg::SB_SELECT_KNOWN_PACKS_ID],
    )
    .await?;
    let packs = cfg::decode_known_packs(&packet.body)?;
    if !packs.contains(&cfg::core_pack()) {
        let reason = text::plain(&format!(
            "이 서버는 Minecraft {} 클라이언트의 기본 데이터가 필요합니다. 클라이언트 버전을 {}(으)로 \
             골라 주세요.",
            crate::protocol::VERSION_NAME,
            crate::protocol::VERSION_NAME
        ));
        stream
            .write_packet(cfg::DISCONNECT_ID, &text::to_nbt(&reason))
            .await?;
        return Ok(None);
    }

    for body in cfg::encode_registries() {
        stream.write_packet(cfg::REGISTRY_DATA_ID, &body).await?;
    }
    stream
        .write_packet(cfg::UPDATE_TAGS_ID, &cfg::encode_tags())
        .await?;
    if !settings.pack_loaded
        && let Some((uuid, url, hash, forced)) = crate::resource_pack::offer(&settings.origin)
    {
        let prompt = text::colored("담비 서버 아이템 그림(복권 등)을 받습니다", "gold");
        stream
            .write_packet(
                cfg::ADD_RESOURCE_PACK_ID,
                &cfg::encode_add_resource_pack(uuid, &url, &hash, forced, Some(&prompt)),
            )
            .await?;
        // 끝난 답(적용·거절·실패)이 올 때까지
        let started = Instant::now();
        loop {
            let left = PACK_LIMIT.saturating_sub(started.elapsed());
            let packet = wait(stream, &mut settings, left, &[cfg::SB_RESOURCE_PACK_ID]).await?;
            let (_, result) = cfg::decode_resource_pack_response(&packet.body)?;
            match result {
                pack_status::LOADED => {
                    settings.pack_loaded = true;
                    settings.pack_problem = None;
                    break;
                }
                pack_status::ACCEPTED | pack_status::DOWNLOADED => {}
                pack_status::DECLINED if forced => {
                    let reason = text::plain(
                        "이 서버는 리소스 팩이 필요합니다. 서버 목록에서 이 서버를 고른 뒤 \
                         '편집 → 서버 리소스 팩: 사용'으로 바꿔 주세요.",
                    );
                    stream
                        .write_packet(cfg::DISCONNECT_ID, &text::to_nbt(&reason))
                        .await?;
                    return Ok(None);
                }
                pack_status::DECLINED => break,
                // 받기·적용 실패는 우리 쪽(주소·네트워크)이나 클라이언트 사정일 수 있으므로
                // 내보내지 않고 팩 없이 들어갑니다 (HUD 등은 안 보임).
                other => {
                    println!(
                        "리소스 팩 {}: {url} (팩 없이 들어감)",
                        pack_status::name(other)
                    );
                    settings.pack_problem = Some(pack_status::name(other).to_owned());
                    break;
                }
            }
        }
    }
    stream
        .write_packet(cfg::FINISH_CONFIGURATION_ID, &[])
        .await?;
    wait(
        stream,
        &mut settings,
        REPLY_LIMIT,
        &[cfg::FINISH_CONFIGURATION_ACK_ID],
    )
    .await?;
    Ok(Some(settings))
}

/// `want` 가운데 하나가 올 때까지 기다립니다. 그 사이의 Client Information은 반영하고, 그 밖의
/// 패킷은 건너뜁니다. 오래 조용하면 keep-alive를 보내고, `limit`가 지나면 시간 초과.
async fn wait<S>(
    stream: &mut Conn<S>,
    settings: &mut ClientSettings,
    limit: Duration,
    want: &[i32],
) -> Result<RawPacket, FrameError>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let started = Instant::now();
    loop {
        let left = limit.saturating_sub(started.elapsed());
        if left.is_zero() {
            return Err(FrameError::Io(std::io::ErrorKind::TimedOut.into()));
        }
        // 읽기는 중간에 멈춰도 읽은 바이트가 버퍼에 남으므로 다음에 이어 읽습니다.
        let packet =
            match tokio::time::timeout(left.min(KEEP_ALIVE_EVERY), stream.read_packet()).await {
                Ok(r) => r?.ok_or(FrameError::Io(std::io::ErrorKind::UnexpectedEof.into()))?,
                Err(_) => {
                    let id = started.elapsed().as_millis() as i64;
                    stream
                        .write_packet(cfg::KEEP_ALIVE_ID, &id.to_be_bytes())
                        .await?;
                    continue;
                }
            };
        if want.contains(&packet.id) {
            return Ok(packet);
        }
        if packet.id == cfg::CLIENT_INFORMATION_ID {
            // 모양이 조금 달라도(모드) 접속은 이어 갑니다
            let _ = cfg::apply_client_information(&packet.body, settings);
        }
        // 그 밖(keep-alive 응답, brand·플러그인 채널, 쿠키, 대화 상자, 행동 강령 동의, 늦게 온
        // 리소스 팩 응답 …)은 건너뜀
    }
}
