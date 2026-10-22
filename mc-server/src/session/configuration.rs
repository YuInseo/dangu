use tokio::io::{AsyncRead, AsyncWrite};

use super::ClientSettings;
use crate::connection::{next, unknown};
use crate::packets::configuration as cfg;
use crate::protocol::frame::{FrameError, write_packet};
use crate::text;

/// 기본 시야 거리. Client Information이 오기 전이거나 안 오면 이것을 씁니다.
const DEFAULT_VIEW_DISTANCE: u8 = 8;

/// 레지스트리·태그를 보내고 Play로 넘길 준비를 합니다. 클라이언트가 우리 known pack을
/// 갖고 있지 않으면 사유를 보내고 `None`.
pub async fn run<S>(stream: &mut S) -> Result<Option<ClientSettings>, FrameError>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let mut settings = ClientSettings {
        view_distance: DEFAULT_VIEW_DISTANCE,
    };

    write_packet(stream, cfg::CUSTOM_PAYLOAD_ID, &cfg::encode_brand("dangu")).await?;
    write_packet(
        stream,
        cfg::UPDATE_ENABLED_FEATURES_ID,
        &cfg::encode_enabled_features(),
    )
    .await?;
    write_packet(
        stream,
        cfg::SELECT_KNOWN_PACKS_ID,
        &cfg::encode_known_packs(&[cfg::core_pack()]),
    )
    .await?;

    // 클라이언트는 Client Information과 brand를 먼저(또는 사이사이) 보낼 수 있습니다.
    loop {
        let packet = next(stream).await?.ok_or(eof())?;
        match packet.id {
            cfg::CLIENT_INFORMATION_ID => {
                settings.view_distance = cfg::decode_view_distance(&packet.body)?;
            }
            cfg::SB_CUSTOM_PAYLOAD_ID | cfg::SB_KEEP_ALIVE_ID => {}
            cfg::SB_SELECT_KNOWN_PACKS_ID => {
                let packs = cfg::decode_known_packs(&packet.body)?;
                if !packs.contains(&cfg::core_pack()) {
                    let reason = text::plain(&format!(
                        "이 서버는 Minecraft {} 클라이언트의 기본 데이터가 필요합니다.",
                        crate::protocol::VERSION_NAME
                    ));
                    write_packet(stream, cfg::DISCONNECT_ID, &text::to_nbt(&reason)).await?;
                    return Ok(None);
                }
                break;
            }
            id => return Err(unknown("configuration", id)),
        }
    }

    for body in cfg::encode_registries() {
        write_packet(stream, cfg::REGISTRY_DATA_ID, &body).await?;
    }
    write_packet(stream, cfg::UPDATE_TAGS_ID, &cfg::encode_tags()).await?;
    write_packet(stream, cfg::FINISH_CONFIGURATION_ID, &[]).await?;

    loop {
        let packet = next(stream).await?.ok_or(eof())?;
        match packet.id {
            cfg::FINISH_CONFIGURATION_ACK_ID => return Ok(Some(settings)),
            cfg::CLIENT_INFORMATION_ID => {
                settings.view_distance = cfg::decode_view_distance(&packet.body)?;
            }
            cfg::SB_CUSTOM_PAYLOAD_ID | cfg::SB_KEEP_ALIVE_ID => {}
            id => return Err(unknown("configuration", id)),
        }
    }
}

fn eof() -> FrameError {
    FrameError::Io(std::io::ErrorKind::UnexpectedEof.into())
}
