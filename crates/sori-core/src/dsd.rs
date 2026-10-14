//! DSD 파일(DSF, DSDIFF)의 머리. 태그 파서가 모르는 형식이라 직접 읽는다.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

#[derive(Clone, Debug, PartialEq)]
pub struct DsdInfo {
    /// 1비트 샘플레이트. DSD64 = 2,822,400Hz.
    pub rate: u32,
    pub channels: u32,
    /// 채널당 1비트 샘플 수.
    pub samples: u64,
    /// 오디오 바이트가 시작하는 곳과 길이.
    pub data_offset: u64,
    pub data_len: u64,
    /// DSF: 채널마다 이만큼씩 번갈아 놓인다(보통 4096). DSDIFF: 1바이트씩 번갈아.
    pub block_size: u32,
    /// DSF는 바이트 안의 비트 순서가 LSB가 먼저다.
    pub lsb_first: bool,
    /// 파일 끝에 붙은 ID3v2 태그(있으면).
    pub id3: Option<(u64, u64)>,
}

impl DsdInfo {
    pub fn duration(&self) -> f64 {
        if self.rate == 0 { 0.0 } else { self.samples as f64 / self.rate as f64 }
    }
}

fn u32le(b: &[u8]) -> u32 {
    u32::from_le_bytes(b[..4].try_into().unwrap())
}
fn u64le(b: &[u8]) -> u64 {
    u64::from_le_bytes(b[..8].try_into().unwrap())
}
fn u64be(b: &[u8]) -> u64 {
    u64::from_be_bytes(b[..8].try_into().unwrap())
}

pub fn probe(path: &Path) -> std::io::Result<DsdInfo> {
    let mut f = File::open(path)?;
    let mut head = [0u8; 4];
    f.read_exact(&mut head)?;
    f.seek(SeekFrom::Start(0))?;
    match &head {
        b"DSD " => probe_dsf(&mut f),
        b"FRM8" => probe_dff(&mut f),
        _ => Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "DSD 파일이 아닙니다")),
    }
}

fn probe_dsf(f: &mut File) -> std::io::Result<DsdInfo> {
    let len = f.metadata()?.len();
    let mut dsd = [0u8; 28];
    f.read_exact(&mut dsd)?;
    let meta = u64le(&dsd[20..]);
    let mut fmt = [0u8; 52];
    f.read_exact(&mut fmt)?;
    if &fmt[..4] != b"fmt " {
        return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "DSF fmt 청크가 없습니다"));
    }
    let channels = u32le(&fmt[24..]);
    let rate = u32le(&fmt[28..]);
    let samples = u64le(&fmt[36..]);
    let block = u32le(&fmt[44..]);
    let fmt_size = u64le(&fmt[4..]);
    f.seek(SeekFrom::Start(28 + fmt_size))?;
    let mut data = [0u8; 12];
    f.read_exact(&mut data)?;
    if &data[..4] != b"data" {
        return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "DSF data 청크가 없습니다"));
    }
    let data_len = u64le(&data[4..]).saturating_sub(12);
    let data_offset = 28 + fmt_size + 12;
    let id3 = (meta > 0 && meta < len).then(|| (meta, len - meta));
    Ok(DsdInfo { rate, channels, samples, data_offset, data_len, block_size: block.max(1), lsb_first: true, id3 })
}

fn probe_dff(f: &mut File) -> std::io::Result<DsdInfo> {
    let len = f.metadata()?.len();
    let mut frm = [0u8; 16];
    f.read_exact(&mut frm)?;
    if &frm[12..16] != b"DSD " {
        return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "DSDIFF가 아닙니다"));
    }
    let (mut rate, mut channels, mut data_offset, mut data_len, mut id3, mut dst) = (0u32, 0u32, 0u64, 0u64, None, false);
    let mut pos = 16u64;
    while pos + 12 <= len {
        f.seek(SeekFrom::Start(pos))?;
        let mut ck = [0u8; 12];
        f.read_exact(&mut ck)?;
        let size = u64be(&ck[4..]);
        let body = pos + 12;
        match &ck[..4] {
            b"PROP" => {
                // "SND " 다음에 하위 청크들
                let mut sub = body + 4;
                while sub + 12 <= body + size {
                    f.seek(SeekFrom::Start(sub))?;
                    let mut h = [0u8; 12];
                    f.read_exact(&mut h)?;
                    let s = u64be(&h[4..]);
                    match &h[..4] {
                        b"FS  " => {
                            let mut b = [0u8; 4];
                            f.read_exact(&mut b)?;
                            rate = u32::from_be_bytes(b);
                        }
                        b"CHNL" => {
                            let mut b = [0u8; 2];
                            f.read_exact(&mut b)?;
                            channels = u16::from_be_bytes(b) as u32;
                        }
                        b"CMPR" => {
                            let mut b = [0u8; 4];
                            f.read_exact(&mut b)?;
                            dst = &b == b"DST ";
                        }
                        _ => {}
                    }
                    sub += 12 + s + (s & 1);
                }
            }
            b"DSD " => {
                data_offset = body;
                data_len = size;
            }
            b"ID3 " => id3 = Some((body, size)),
            _ => {}
        }
        pos = body + size + (size & 1);
    }
    if dst {
        return Err(std::io::Error::new(std::io::ErrorKind::Unsupported, "DST로 압축된 DSDIFF는 아직 못 엽니다"));
    }
    if channels == 0 || rate == 0 || data_offset == 0 {
        return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "DSDIFF 머리가 모자랍니다"));
    }
    let samples = data_len * 8 / channels as u64;
    Ok(DsdInfo { rate, channels, samples, data_offset, data_len, block_size: 1, lsb_first: false, id3 })
}

/// ID3v2 태그를 감싼 작은 WAV를 만든다. 태그 파서가 DSD는 몰라도 WAV의 `id3 ` 청크는 읽는다.
pub fn id3_as_wav(path: &Path, info: &DsdInfo) -> Option<Vec<u8>> {
    let (off, len) = info.id3?;
    if len > 64 * 1024 * 1024 {
        return None;
    }
    let mut f = File::open(path).ok()?;
    f.seek(SeekFrom::Start(off)).ok()?;
    let mut tag = vec![0u8; len as usize];
    f.read_exact(&mut tag).ok()?;
    if !tag.starts_with(b"ID3") {
        return None;
    }
    if tag.len() % 2 == 1 {
        tag.push(0);
    }
    let mut w = Vec::with_capacity(tag.len() + 64);
    let riff_len = 4 + (8 + 16) + (8 + 4) + (8 + tag.len());
    w.extend_from_slice(b"RIFF");
    w.extend_from_slice(&(riff_len as u32).to_le_bytes());
    w.extend_from_slice(b"WAVE");
    w.extend_from_slice(b"fmt ");
    w.extend_from_slice(&16u32.to_le_bytes());
    w.extend_from_slice(&1u16.to_le_bytes()); // PCM
    w.extend_from_slice(&(info.channels.max(1) as u16).to_le_bytes());
    w.extend_from_slice(&44_100u32.to_le_bytes());
    w.extend_from_slice(&(44_100u32 * 2 * info.channels.max(1)).to_le_bytes());
    w.extend_from_slice(&((2 * info.channels.max(1)) as u16).to_le_bytes());
    w.extend_from_slice(&16u16.to_le_bytes());
    w.extend_from_slice(b"data");
    w.extend_from_slice(&4u32.to_le_bytes());
    w.extend_from_slice(&[0, 0, 0, 0]);
    w.extend_from_slice(b"id3 ");
    w.extend_from_slice(&(tag.len() as u32).to_le_bytes());
    w.extend_from_slice(&tag);
    Some(w)
}
