//! Opus(.opus, .webm, 그리고 .ogg 안의 Opus). 유튜브에서 받은 음악이 흔히 이 형식인데 symphonia는
//! 풀지 못하므로, 공식 시험 벡터를 다 통과하는 순수 러스트 디코더로 푼다.
//!
//! Opus는 늘 48kHz로 풀린다. 앞의 pre-skip과 끝의 패딩은 컨테이너가 패킷마다 알려 준 만큼
//! (`trim_start`, `trim_end`) 부르는 쪽이 잘라 길이가 정확하다(앨범을 이어 틀 때 틈이 없다).

use anyhow::{anyhow, bail};
use opus_decoder::{OpusDecoder, OpusMultistreamDecoder};

/// 한 패킷의 최대 길이(120ms, 48kHz)
const MAX_FRAMES: usize = 5760;

enum Inner {
    /// 모노·스테레오(채널 배치 0)
    Simple(Box<OpusDecoder>),
    /// 서라운드 등 여러 스트림(채널 배치 1 이상)
    Multi(OpusMultistreamDecoder),
}

pub struct OpusDec {
    inner: Inner,
    channels: usize,
    /// OpusHead의 출력 게인(선형)
    gain: f64,
    /// OpusHead의 pre-skip(48kHz 프레임). 컨테이너가 알려 주지 않을 때 쓴다(WebM).
    pre_skip: Option<u64>,
    pcm: Vec<f32>,
}

/// OpusHead에서 읽은 것
#[derive(Debug, PartialEq)]
struct Head {
    channels: usize,
    pre_skip: u16,
    gain_db: f64,
    family: u8,
    streams: usize,
    coupled: usize,
    mapping: Vec<u8>,
}

/// RFC 7845 5.1의 식별 헤더. 없거나 모양이 다르면 None.
fn parse_head(h: &[u8]) -> Option<Head> {
    if h.len() < 19 || &h[..8] != b"OpusHead" {
        return None;
    }
    let channels = h[9] as usize;
    let pre_skip = u16::from_le_bytes([h[10], h[11]]);
    let gain_db = f64::from(i16::from_le_bytes([h[16], h[17]])) / 256.0;
    let family = h[18];
    if family == 0 {
        return Some(Head { channels, pre_skip, gain_db, family, streams: 1, coupled: usize::from(channels >= 2), mapping: Vec::new() });
    }
    if h.len() < 21 + channels {
        return None;
    }
    Some(Head {
        channels,
        pre_skip,
        gain_db,
        family,
        streams: h[19] as usize,
        coupled: h[20] as usize,
        mapping: h[21..21 + channels].to_vec(),
    })
}

impl OpusDec {
    /// `channels`는 컨테이너가 알려 준 채널 수, `head`는 코덱의 덧붙은 자료(OpusHead).
    pub fn new(channels: usize, head: Option<&[u8]>) -> anyhow::Result<Self> {
        let head = head.and_then(parse_head);
        let channels = head.as_ref().map(|h| h.channels).filter(|&c| c > 0).unwrap_or(channels).max(1);
        let gain_db = head.as_ref().map(|h| h.gain_db).unwrap_or(0.0);
        let pre_skip = head.as_ref().map(|h| u64::from(h.pre_skip));
        let inner = match head {
            Some(h) if h.family != 0 => Inner::Multi(
                OpusMultistreamDecoder::new(48_000, channels, h.streams, h.coupled, &h.mapping).map_err(|e| anyhow!("Opus: {e}"))?,
            ),
            _ => {
                if channels > 2 {
                    bail!("Opus: 채널 배치 정보 없이 {channels}채널입니다");
                }
                Inner::Simple(Box::new(OpusDecoder::new(48_000, channels).map_err(|e| anyhow!("Opus: {e}"))?))
            }
        };
        Ok(OpusDec { inner, channels, gain: 10f64.powf(gain_db / 20.0), pre_skip, pcm: vec![0.0; MAX_FRAMES * channels] })
    }

    pub fn channels(&self) -> usize {
        self.channels
    }

    pub fn pre_skip(&self) -> Option<u64> {
        self.pre_skip
    }

    /// 패킷 하나를 풀어 `out`(인터리브) 뒤에 붙이고, 붙인 프레임 수를 돌려준다.
    pub fn decode(&mut self, packet: &[u8], out: &mut Vec<f64>) -> anyhow::Result<usize> {
        let n = match &mut self.inner {
            Inner::Simple(d) => d.decode_float(packet, &mut self.pcm, false),
            Inner::Multi(d) => d.decode_float(packet, &mut self.pcm, false),
        }
        .map_err(|e| anyhow!("Opus: {e}"))?;
        let samples = &self.pcm[..n * self.channels];
        out.reserve(samples.len());
        if self.gain == 1.0 {
            out.extend(samples.iter().map(|&s| f64::from(s)));
        } else {
            out.extend(samples.iter().map(|&s| f64::from(s) * self.gain));
        }
        Ok(n)
    }

    /// 시크한 뒤: 앞 패킷에 기대는 상태를 비운다.
    pub fn reset(&mut self) {
        match &mut self.inner {
            Inner::Simple(d) => d.reset(),
            Inner::Multi(d) => d.reset(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn head(channels: u8, gain: i16, family: u8, tail: &[u8]) -> Vec<u8> {
        let mut h = b"OpusHead".to_vec();
        h.push(1); // 버전
        h.push(channels);
        h.extend_from_slice(&312u16.to_le_bytes()); // pre-skip
        h.extend_from_slice(&48_000u32.to_le_bytes());
        h.extend_from_slice(&gain.to_le_bytes());
        h.push(family);
        h.extend_from_slice(tail);
        h
    }

    #[test]
    fn reads_the_identification_header() {
        let h = parse_head(&head(2, -256, 0, &[])).unwrap();
        assert_eq!((h.channels, h.family, h.streams, h.coupled, h.pre_skip), (2, 0, 1, 1, 312));
        assert!((h.gain_db + 1.0).abs() < 1e-12, "Q7.8 dB: -256 → -1dB");
        // 5.1: 스트림 넷, 그중 둘은 스테레오, 그리고 채널마다 어느 스트림인지
        let h = parse_head(&head(6, 0, 1, &[4, 2, 0, 4, 1, 2, 3, 5])).unwrap();
        assert_eq!((h.streams, h.coupled, h.mapping.len()), (4, 2, 6));
        assert!(parse_head(b"OpusTags........").is_none());
        assert!(parse_head(&head(6, 0, 1, &[4, 2, 0])).is_none(), "배치 표가 모자라다");
    }

    #[test]
    fn builds_decoders_for_stereo_and_surround() {
        assert_eq!(OpusDec::new(2, Some(&head(2, 0, 0, &[]))).unwrap().channels(), 2);
        assert_eq!(OpusDec::new(2, None).unwrap().channels(), 2, "헤더가 없으면 컨테이너의 채널 수로");
        assert_eq!(OpusDec::new(6, Some(&head(6, 0, 1, &[4, 2, 0, 4, 1, 2, 3, 5]))).unwrap().channels(), 6);
        assert!(OpusDec::new(6, None).is_err());
    }
}
