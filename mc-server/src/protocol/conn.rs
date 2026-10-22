//! 연결 하나의 패킷 입출력: 프레임 + 압축(Set Compression 이후) + 암호화(정품 인증 이후).
//!
//! ```text
//! 평문:     VarInt 길이 | VarInt ID | 데이터
//! 압축 후:  VarInt 길이 | VarInt 원래 길이(0 = 압축 안 함) | (zlib) ID + 데이터
//! 암호화:   위 바이트 전체를 AES-128-CFB8로
//! ```
//!
//! 읽기와 쓰기는 따로 움직이므로(Play에서는 다른 태스크) 반쪽씩 나눠 둡니다. 암호 상태도
//! 방향마다 따로입니다.

use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, ReadHalf, WriteHalf};

use super::DecodeError;
use super::frame::{FrameError, MAX_PACKET_LEN, RawPacket};
use super::varint::{VARINT_MAX_LEN, read_varint, varint_len, write_varint};
use super::zlib;
use crate::crypto::aes::Cfb8;

pub struct PacketReader<R> {
    inner: R,
    cipher: Option<Cfb8>,
    threshold: Option<usize>,
    /// 복호화한, 아직 쓰지 않은 바이트는 `buf[start..]`.
    buf: Vec<u8>,
    start: usize,
}

pub struct PacketWriter<W> {
    inner: W,
    cipher: Option<Cfb8>,
    threshold: Option<usize>,
}

impl<R: AsyncRead + Unpin> PacketReader<R> {
    pub fn new(inner: R) -> Self {
        Self {
            inner,
            cipher: None,
            threshold: None,
            buf: Vec::new(),
            start: 0,
        }
    }

    pub fn enable_encryption(&mut self, shared_secret: &[u8; 16]) {
        // 이미 읽어 둔 바이트는 암호화 전 것이어야 합니다 (클라이언트는 응답을 보낸 뒤에야
        // 암호화를 켜므로 버퍼는 비어 있습니다).
        debug_assert_eq!(
            self.start,
            self.buf.len(),
            "unread plaintext before encryption"
        );
        self.cipher = Some(Cfb8::new(shared_secret));
    }

    pub fn set_compression(&mut self, threshold: Option<usize>) {
        self.threshold = threshold;
    }

    /// 버퍼에 `n`바이트가 있도록 채웁니다. 연결이 먼저 끝나면 false.
    async fn fill(&mut self, n: usize) -> std::io::Result<bool> {
        if self.start > 0 && self.start == self.buf.len() {
            self.buf.clear();
            self.start = 0;
        }
        while self.buf.len() - self.start < n {
            if self.start > 64 * 1024 {
                self.buf.drain(..self.start);
                self.start = 0;
            }
            let mut chunk = [0u8; 16 * 1024];
            let read = self.inner.read(&mut chunk).await?;
            if read == 0 {
                return Ok(false);
            }
            if let Some(c) = &mut self.cipher {
                c.decrypt(&mut chunk[..read]);
            }
            self.buf.extend_from_slice(&chunk[..read]);
        }
        Ok(true)
    }

    pub async fn peek_byte(&mut self) -> std::io::Result<Option<u8>> {
        Ok(if self.fill(1).await? {
            Some(self.buf[self.start])
        } else {
            None
        })
    }

    /// 패킷 하나. 패킷 경계에서 연결이 닫혔으면 `Ok(None)`.
    pub async fn read_packet(&mut self) -> Result<Option<RawPacket>, FrameError> {
        // 길이 VarInt: 바이트가 몇 개일지 모르므로 하나씩
        let mut len_bytes = 0;
        loop {
            if !self.fill(len_bytes + 1).await? {
                if len_bytes == 0 {
                    return Ok(None);
                }
                return Err(FrameError::Io(std::io::ErrorKind::UnexpectedEof.into()));
            }
            let b = self.buf[self.start + len_bytes];
            len_bytes += 1;
            if b & 0x80 == 0 {
                break;
            }
            if len_bytes == VARINT_MAX_LEN {
                return Err(DecodeError::VarIntTooLong.into());
            }
        }
        let len = read_varint(&mut &self.buf[self.start..self.start + len_bytes])?;
        if !(1..=MAX_PACKET_LEN).contains(&len) {
            return Err(DecodeError::BadPacketLength(len).into());
        }
        if !self.fill(len_bytes + len as usize).await? {
            return Err(FrameError::Io(std::io::ErrorKind::UnexpectedEof.into()));
        }
        let frame_start = self.start + len_bytes;
        let frame = &self.buf[frame_start..frame_start + len as usize];
        self.start = frame_start + len as usize;

        let mut buf: &[u8] = frame;
        let inflated;
        if let Some(threshold) = self.threshold {
            let data_len = read_varint(&mut buf)?;
            if data_len != 0 {
                if (data_len as usize) < threshold || data_len as usize > zlib::MAX_INFLATED {
                    return Err(DecodeError::InvalidValue("compressed length").into());
                }
                inflated = zlib::decompress(buf, data_len as usize)
                    .map_err(|_| DecodeError::InvalidValue("zlib data"))?;
                if inflated.len() != data_len as usize {
                    return Err(DecodeError::InvalidValue("inflated length").into());
                }
                buf = &inflated;
            }
        }
        let id = read_varint(&mut buf)?;
        Ok(Some(RawPacket {
            id,
            body: buf.to_vec(),
        }))
    }
}

impl<W: AsyncWrite + Unpin> PacketWriter<W> {
    pub fn new(inner: W) -> Self {
        Self {
            inner,
            cipher: None,
            threshold: None,
        }
    }

    pub fn enable_encryption(&mut self, shared_secret: &[u8; 16]) {
        self.cipher = Some(Cfb8::new(shared_secret));
    }

    pub fn set_compression(&mut self, threshold: Option<usize>) {
        self.threshold = threshold;
    }

    pub fn encode(&self, id: i32, body: &[u8]) -> Vec<u8> {
        let mut packet = Vec::with_capacity(varint_len(id) + body.len());
        write_varint(&mut packet, id);
        packet.extend_from_slice(body);

        let mut out = Vec::with_capacity(packet.len() + 8);
        match self.threshold {
            None => {
                write_varint(&mut out, packet.len() as i32);
                out.extend_from_slice(&packet);
            }
            Some(t) if packet.len() < t => {
                write_varint(&mut out, packet.len() as i32 + 1);
                out.push(0); // 압축 안 함
                out.extend_from_slice(&packet);
            }
            Some(_) => {
                let compressed = zlib::compress(&packet);
                write_varint(
                    &mut out,
                    (varint_len(packet.len() as i32) + compressed.len()) as i32,
                );
                write_varint(&mut out, packet.len() as i32);
                out.extend_from_slice(&compressed);
            }
        }
        out
    }

    pub async fn write_packet(&mut self, id: i32, body: &[u8]) -> std::io::Result<()> {
        let mut bytes = self.encode(id, body);
        if let Some(c) = &mut self.cipher {
            c.encrypt(&mut bytes);
        }
        self.inner.write_all(&bytes).await?;
        self.inner.flush().await
    }

    pub async fn shutdown(&mut self) -> std::io::Result<()> {
        self.inner.shutdown().await
    }
}

/// 읽기·쓰기 반쪽 한 쌍. Play에 들어가면 [`Conn::into_split`]으로 나눕니다.
pub struct Conn<S> {
    pub r: PacketReader<ReadHalf<S>>,
    pub w: PacketWriter<WriteHalf<S>>,
}

impl<S: AsyncRead + AsyncWrite> Conn<S> {
    pub fn new(stream: S) -> Self {
        let (r, w) = tokio::io::split(stream);
        Self {
            r: PacketReader::new(r),
            w: PacketWriter::new(w),
        }
    }
}

impl<S: AsyncRead + AsyncWrite + Unpin> Conn<S> {
    pub async fn read_packet(&mut self) -> Result<Option<RawPacket>, FrameError> {
        self.r.read_packet().await
    }

    pub async fn write_packet(&mut self, id: i32, body: &[u8]) -> std::io::Result<()> {
        self.w.write_packet(id, body).await
    }

    pub fn enable_encryption(&mut self, shared_secret: &[u8; 16]) {
        self.r.enable_encryption(shared_secret);
        self.w.enable_encryption(shared_secret);
    }

    pub fn set_compression(&mut self, threshold: Option<usize>) {
        self.r.set_compression(threshold);
        self.w.set_compression(threshold);
    }

    pub fn into_split(self) -> (PacketReader<ReadHalf<S>>, PacketWriter<WriteHalf<S>>) {
        (self.r, self.w)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn round_trip(threshold: Option<usize>, secret: Option<[u8; 16]>) {
        let (a, b) = tokio::io::duplex(1 << 20);
        let (mut client, mut server) = (Conn::new(a), Conn::new(b));
        client.set_compression(threshold);
        server.set_compression(threshold);
        if let Some(s) = secret {
            client.enable_encryption(&s);
            server.enable_encryption(&s);
        }
        let big: Vec<u8> = (0..50_000u32).map(|i| (i % 13) as u8).collect();
        let packets: Vec<(i32, Vec<u8>)> = vec![
            (0, vec![]),
            (0x2E, big),
            (0x7C, b"small".to_vec()),
            (300, vec![1; 255]),
        ];
        for (id, body) in &packets {
            client.write_packet(*id, body).await.unwrap();
        }
        for (id, body) in &packets {
            let p = server.read_packet().await.unwrap().unwrap();
            assert_eq!((p.id, &p.body), (*id, body));
        }
    }

    #[tokio::test]
    async fn plain_compressed_encrypted_and_both() {
        round_trip(None, None).await;
        round_trip(Some(256), None).await;
        round_trip(None, Some([7; 16])).await;
        round_trip(Some(64), Some(crate::crypto::random::bytes::<16>())).await;
    }

    #[tokio::test]
    async fn rejects_undersized_compressed_packets() {
        // 압축했다고 하면서 원래 길이가 기준보다 작으면 거부 (바닐라와 같음)
        let (a, b) = tokio::io::duplex(4096);
        let mut server = Conn::new(b);
        server.set_compression(Some(256));
        let mut raw = Vec::new();
        let z = zlib::compress(&[0, 1, 2]);
        write_varint(&mut raw, (1 + z.len()) as i32);
        write_varint(&mut raw, 3);
        raw.extend_from_slice(&z);
        let (_, mut w) = tokio::io::split(a);
        w.write_all(&raw).await.unwrap();
        assert!(server.read_packet().await.is_err());
    }
}
