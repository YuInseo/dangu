use tokio::io::{AsyncRead, AsyncWrite};

use super::Profile;
use crate::connection::{next, unknown};
use crate::packets::login::{self, Hello};
use crate::protocol::frame::{FrameError, write_packet};
use crate::uuid::{format_uuid, offline_uuid, random_uuid};

/// Hello → Login Finished → Login Acknowledged.
pub async fn run<S>(stream: &mut S) -> Result<Profile, FrameError>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let packet = next(stream).await?.ok_or(eof())?;
    if packet.id != login::HELLO_ID {
        return Err(unknown("login", packet.id));
    }
    let hello = Hello::decode(&packet.body)?;
    let uuid = offline_uuid(&hello.name);
    println!("{} 로그인 ({})", hello.name, format_uuid(uuid));

    let finished = login::encode_login_finished(uuid, &hello.name, random_uuid());
    write_packet(stream, login::LOGIN_FINISHED_ID, &finished).await?;

    let packet = next(stream).await?.ok_or(eof())?;
    if packet.id != login::LOGIN_ACKNOWLEDGED_ID {
        return Err(unknown("login", packet.id));
    }
    Ok(Profile {
        name: hello.name,
        uuid,
    })
}

fn eof() -> FrameError {
    FrameError::Io(std::io::ErrorKind::UnexpectedEof.into())
}
