use tokio::io::{AsyncRead, AsyncWrite};

use super::ClientSettings;
use crate::connection::{next, unknown};
use crate::packets::configuration as cfg;
use crate::protocol::conn::Conn;
use crate::protocol::frame::FrameError;
use crate::text;

/// 기본 시야 거리. Client Information이 오기 전이거나 안 오면 이것을 씁니다.
pub(super) const DEFAULT_VIEW_DISTANCE: u8 = 8;

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

    // 클라이언트는 Client Information과 brand를 먼저(또는 사이사이) 보낼 수 있습니다.
    loop {
        let packet = next(stream).await?.ok_or(eof())?;
        match packet.id {
            cfg::CLIENT_INFORMATION_ID => {
                cfg::apply_client_information(&packet.body, &mut settings)?;
            }
            cfg::SB_CUSTOM_PAYLOAD_ID | cfg::SB_KEEP_ALIVE_ID => {}
            cfg::SB_SELECT_KNOWN_PACKS_ID => {
                let packs = cfg::decode_known_packs(&packet.body)?;
                if !packs.contains(&cfg::core_pack()) {
                    let reason = text::plain(&format!(
                        "이 서버는 Minecraft {} 클라이언트의 기본 데이터가 필요합니다.",
                        crate::protocol::VERSION_NAME
                    ));
                    stream
                        .write_packet(cfg::DISCONNECT_ID, &text::to_nbt(&reason))
                        .await?;
                    return Ok(None);
                }
                break;
            }
            id => return Err(unknown("configuration", id)),
        }
    }

    for body in cfg::encode_registries() {
        stream.write_packet(cfg::REGISTRY_DATA_ID, &body).await?;
    }
    stream
        .write_packet(cfg::UPDATE_TAGS_ID, &cfg::encode_tags())
        .await?;
    stream
        .write_packet(cfg::FINISH_CONFIGURATION_ID, &[])
        .await?;

    loop {
        let packet = next(stream).await?.ok_or(eof())?;
        match packet.id {
            cfg::FINISH_CONFIGURATION_ACK_ID => return Ok(Some(settings)),
            cfg::CLIENT_INFORMATION_ID => {
                cfg::apply_client_information(&packet.body, &mut settings)?;
            }
            cfg::SB_CUSTOM_PAYLOAD_ID | cfg::SB_KEEP_ALIVE_ID => {}
            id => return Err(unknown("configuration", id)),
        }
    }
}

fn eof() -> FrameError {
    FrameError::Io(std::io::ErrorKind::UnexpectedEof.into())
}
