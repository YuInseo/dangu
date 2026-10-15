//! 파일 → 64비트 부동소수점 PCM.
//!
//! 정수 PCM(16/24/32비트)은 2의 거듭제곱으로 나눠 옮기므로 f64로 바꿔도 한 비트도
//! 잃지 않는다. 테스트가 ffmpeg로 푼 정수와 비트 단위로 같은지 본다.
//! DSD는 symphonia가 모르므로 여기서 직접 PCM 176.4kHz로 바꾼다.

use std::fs::File;
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::path::Path;

use anyhow::{Context, anyhow, bail};
use symphonia::core::codecs::audio::well_known::CODEC_ID_OPUS;
use symphonia::core::codecs::audio::{AudioCodecParameters, AudioDecoder, AudioDecoderOptions};
use symphonia::core::errors::Error as SymError;
use symphonia::core::formats::probe::Hint;
use symphonia::core::formats::{FormatOptions, FormatReader, SeekMode, SeekTo, TrackType};
use symphonia::core::io::{MediaSource, MediaSourceStream};
use symphonia::core::meta::MetadataOptions;
use symphonia::core::units::Time;

use crate::dsd::{self, DsdInfo};
use crate::dsp::fir::{Decimator, kaiser_beta, kaiser_taps, lowpass};
use crate::opus::OpusDec;

#[derive(Clone, Debug, PartialEq)]
pub struct Spec {
    pub rate: u32,
    pub channels: usize,
    /// 알면 전체 프레임 수.
    pub frames: Option<u64>,
    /// 원본의 비트 깊이(정수 PCM이면).
    pub bits: Option<u32>,
    /// DSD였다면 원래의 1비트 샘플레이트.
    pub dsd_rate: Option<u32>,
}

impl Spec {
    pub fn duration(&self) -> Option<f64> {
        self.frames.map(|f| f as f64 / self.rate as f64)
    }
}

pub trait PcmSource: Send {
    fn spec(&self) -> &Spec;
    /// 인터리브된 샘플을 `out` 뒤에 붙이고 붙인 프레임 수를 돌려준다. 0이면 끝.
    fn read(&mut self, out: &mut Vec<f64>) -> anyhow::Result<usize>;
    /// 샘플 단위로 정확히 그 프레임으로 간다. 실제로 간 프레임을 돌려준다.
    fn seek(&mut self, frame: u64) -> anyhow::Result<u64>;
    /// 지금 읽어도 막히지 않는지. 네트워크 원본이 아직 받는 중이면 false라서, 재생 스레드는
    /// 기다리는 대신 다른 일(명령)을 보고 다시 온다.
    fn ready(&self) -> bool {
        true
    }
    /// 네트워크에서 받는 원본인지(시크·열기를 재생 스레드 밖에서 한다).
    fn remote(&self) -> bool {
        false
    }
}

/// 엔진이 풀지 못하는 코덱(돌비 E-AC-3 같은 것). 화면은 이때 기기의 디코더로 트는 기본 재생으로 넘긴다.
#[derive(Debug)]
pub struct UnsupportedCodec(pub &'static str);

impl std::fmt::Display for UnsupportedCodec {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} 형식은 소리 엔진이 풀 수 없습니다", self.0)
    }
}

impl std::error::Error for UnsupportedCodec {}

/// symphonia의 코덱 번호 → 화면에 적을 이름
pub fn codec_name(id: symphonia::core::codecs::audio::AudioCodecId) -> &'static str {
    use symphonia::core::codecs::audio::well_known::*;
    match id {
        CODEC_ID_OPUS => "Opus",
        CODEC_ID_VORBIS => "Vorbis",
        CODEC_ID_AAC => "AAC",
        CODEC_ID_FLAC => "FLAC",
        CODEC_ID_ALAC => "ALAC",
        CODEC_ID_MP3 | CODEC_ID_MP2 | CODEC_ID_MP1 => "MP3",
        CODEC_ID_AC3 => "AC-3",
        CODEC_ID_EAC3 => "E-AC-3",
        CODEC_ID_AC4 => "AC-4",
        CODEC_ID_WMA => "WMA",
        CODEC_ID_WAVPACK => "WavPack",
        CODEC_ID_MONKEYS_AUDIO => "APE",
        CODEC_ID_SPEEX => "Speex",
        _ if id < CODEC_ID_VORBIS => "PCM",
        _ => "알 수 없는 코덱",
    }
}

/// 태그를 읽지 못하는 파일(WebM 등)의 형식과 길이. 디코더는 만들지 않는다.
#[derive(Clone, Debug, PartialEq)]
pub struct ContainerInfo {
    pub codec: &'static str,
    pub sample_rate: Option<u32>,
    pub channels: Option<u32>,
    pub bits: Option<u32>,
    /// 초
    pub duration: f64,
}

pub fn probe_container(path: &Path) -> anyhow::Result<ContainerInfo> {
    let ext = path.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
    let file = File::open(path).with_context(|| format!("열 수 없습니다: {}", path.display()))?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());
    let mut hint = Hint::new();
    hint.with_extension(if ext == "weba" { "webm" } else { &ext });
    let mut format = symphonia::default::get_probe()
        .probe(&hint, mss, FormatOptions::default(), MetadataOptions::default())
        .map_err(|e| anyhow!("형식을 알 수 없습니다: {e}"))?;
    let track = format.default_track(TrackType::Audio).ok_or_else(|| anyhow!("오디오 트랙이 없습니다"))?.clone();
    let params = track.codec_params.as_ref().and_then(|p| p.audio()).ok_or_else(|| anyhow!("오디오 코덱 정보가 없습니다"))?;
    let codec = codec_name(params.codec);
    let sample_rate = if codec == "Opus" { Some(48_000) } else { params.sample_rate };
    let duration = match (track.num_frames, sample_rate, track.duration, track.time_base) {
        (Some(n), Some(r), _, _) if r > 0 => n.saturating_sub(u64::from(track.delay.unwrap_or(0))) as f64 / f64::from(r),
        (_, _, Some(d), Some(tb)) => tb.calc_time_saturating(symphonia::core::units::Timestamp::new(d.get() as i64)).as_secs_f64(),
        (_, _, _, Some(tb)) => {
            // 컨테이너가 길이를 알려 주지 않는다(WebM). 끝까지 패킷만 훑어(풀지는 않는다) 마지막 시각을 본다.
            let mut end = 0i64;
            while let Ok(Some(p)) = format.next_packet() {
                if p.track_id == track.id {
                    end = end.max(p.pts.get() + p.dur.get() as i64);
                }
            }
            tb.calc_time_saturating(symphonia::core::units::Timestamp::new(end)).as_secs_f64()
        }
        _ => 0.0,
    };
    Ok(ContainerInfo {
        codec,
        sample_rate,
        channels: params.channels.as_ref().map(|c| c.count() as u32),
        bits: params.bits_per_sample,
        duration,
    })
}

pub fn open(path: &Path) -> anyhow::Result<Box<dyn PcmSource>> {
    let ext = path.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
    if ext == "dsf" || ext == "dff" {
        return Ok(Box::new(DsdSource::open(path)?));
    }
    // .weba는 WebM이다(이름만 다르다).
    let ext = if ext == "weba" { "webm".to_string() } else { ext };
    Ok(Box::new(SymphoniaSource::open(path, &ext)?))
}

/// 파일을 끝까지 풀어 한 번에 돌려준다(테스트, 분석용).
pub fn decode_all(path: &Path) -> anyhow::Result<(Spec, Vec<f64>)> {
    let mut src = open(path)?;
    let mut out = Vec::new();
    while src.read(&mut out)? > 0 {}
    Ok((src.spec().clone(), out))
}

/// Opus를 시크할 때 앞에서부터 풀어 버리는 길이(48kHz 프레임, 250ms)
const OPUS_PREROLL: u64 = 12_000;

/// symphonia의 디코더, 또는 symphonia가 풀지 못하는 Opus를 위한 우리 디코더.
enum Dec {
    Sym(Box<dyn AudioDecoder>),
    Opus(Box<OpusDec>),
}

impl Dec {
    fn new(params: &AudioCodecParameters) -> anyhow::Result<Dec> {
        if params.codec == CODEC_ID_OPUS {
            let channels = params.channels.as_ref().map(|c| c.count()).unwrap_or(2);
            return Ok(Dec::Opus(Box::new(OpusDec::new(channels, params.extra_data.as_deref())?)));
        }
        let d = symphonia::default::get_codecs().make_audio_decoder(params, &AudioDecoderOptions::default()).map_err(|e| match e {
            SymError::Unsupported(_) => anyhow::Error::new(UnsupportedCodec(codec_name(params.codec))),
            e => anyhow!("이 코덱을 풀 준비를 하지 못했습니다: {e}"),
        })?;
        Ok(Dec::Sym(d))
    }

    fn reset(&mut self) {
        match self {
            Dec::Sym(d) => d.reset(),
            Dec::Opus(d) => d.reset(),
        }
    }
}

pub struct SymphoniaSource {
    format: Box<dyn FormatReader>,
    decoder: Dec,
    track_id: u32,
    spec: Spec,
    /// 시크한 뒤 버려야 할 프레임 수.
    skip: u64,
    /// 컨테이너의 시간축에서 소리가 시작하는 곳(Opus의 pre-skip). 이만큼 앞을 버리고, 시크도 이만큼 민다.
    delay: u64,
    /// 처음 패킷을 아직 못 봤다(컨테이너가 앞을 이미 잘라 주는지 거기서 안다).
    first_packet: bool,
    scratch: Vec<f64>,
    /// 네트워크 원본이면 읽어도 되는지 묻는 곳
    gate: Option<Box<dyn Fn() -> bool + Send>>,
}

impl SymphoniaSource {
    pub fn open(path: &Path, ext: &str) -> anyhow::Result<Self> {
        let file = File::open(path).with_context(|| format!("열 수 없습니다: {}", path.display()))?;
        Self::from_media(Box::new(file), ext)
    }

    /// 파일이 아닌 원본(네트워크)에서. `ext`는 형식을 짐작하는 데 쓴다.
    pub fn from_media(media: Box<dyn MediaSource>, ext: &str) -> anyhow::Result<Self> {
        let mss = MediaSourceStream::new(media, Default::default());
        let mut hint = Hint::new();
        if !ext.is_empty() {
            hint.with_extension(ext);
        }
        let format = symphonia::default::get_probe()
            .probe(&hint, mss, FormatOptions::default(), MetadataOptions::default())
            .map_err(|e| anyhow!("형식을 알 수 없습니다: {e}"))?;
        let track = format.default_track(TrackType::Audio).ok_or_else(|| anyhow!("오디오 트랙이 없습니다"))?;
        let params = track.codec_params.as_ref().and_then(|p| p.audio()).ok_or_else(|| anyhow!("오디오 코덱 정보가 없습니다"))?;
        let decoder = Dec::new(params)?;
        // Opus는 무엇으로 담았든 48kHz로 풀린다.
        let rate = match decoder {
            Dec::Opus(_) => 48_000,
            Dec::Sym(_) => params.sample_rate.ok_or_else(|| anyhow!("샘플레이트를 모릅니다"))?,
        };
        let channels = match &decoder {
            Dec::Opus(d) => d.channels(),
            Dec::Sym(_) => params.channels.as_ref().map(|c| c.count()).unwrap_or(2).max(1),
        };
        let spec = Spec {
            rate,
            channels,
            frames: track.num_frames,
            bits: params.bits_per_sample.or(params.bits_per_coded_sample),
            dsd_rate: None,
        };
        let track_id = track.id;
        // Opus의 pre-skip: Ogg는 패킷을 자르지 않고 트랙에만 적어 두고, WebM은 그마저 없어
        // Opus 헤더에 적힌 값을 쓴다. 여기서 버린다.
        let delay = match &decoder {
            Dec::Opus(d) => track.delay.map(u64::from).or(d.pre_skip()).unwrap_or(0),
            Dec::Sym(_) => 0,
        };
        let spec = Spec { frames: spec.frames.map(|n| n.saturating_sub(delay)), ..spec };
        Ok(SymphoniaSource { format, decoder, track_id, spec, skip: delay, delay, first_packet: true, scratch: Vec::new(), gate: None })
    }

    /// 네트워크 원본: `ready`가 false면 재생 스레드가 읽지 않고 기다린다.
    pub fn with_gate(mut self, ready: impl Fn() -> bool + Send + 'static) -> Self {
        self.gate = Some(Box::new(ready));
        self
    }

    fn rebuild_decoder(&mut self) -> anyhow::Result<()> {
        let track = self.format.tracks().iter().find(|t| t.id == self.track_id).ok_or_else(|| anyhow!("트랙이 사라졌습니다"))?;
        let params = track.codec_params.as_ref().and_then(|p| p.audio()).ok_or_else(|| anyhow!("코덱 정보가 없습니다"))?;
        self.decoder = Dec::new(params)?;
        Ok(())
    }
}

impl PcmSource for SymphoniaSource {
    fn spec(&self) -> &Spec {
        &self.spec
    }

    fn ready(&self) -> bool {
        self.gate.as_ref().is_none_or(|g| g())
    }

    fn remote(&self) -> bool {
        self.gate.is_some()
    }

    fn read(&mut self, out: &mut Vec<f64>) -> anyhow::Result<usize> {
        loop {
            let packet = match self.format.next_packet() {
                Ok(Some(p)) => p,
                Ok(None) => return Ok(0),
                Err(SymError::ResetRequired) => {
                    self.rebuild_decoder()?;
                    continue;
                }
                Err(SymError::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(0),
                Err(e) => return Err(anyhow!("읽다가 멈췄습니다: {e}")),
            };
            if packet.track_id != self.track_id {
                continue;
            }
            let (frames, ch) = match &mut self.decoder {
                Dec::Sym(dec) => {
                    let buf = match dec.decode(&packet) {
                        Ok(b) => b,
                        // 깨진 패킷 하나는 건너뛴다. 소리가 아주 잠깐 비어도 멈추는 것보다 낫다.
                        Err(SymError::DecodeError(_)) => continue,
                        Err(e) => return Err(anyhow!("디코딩 실패: {e}")),
                    };
                    let ch = buf.num_planes().max(1);
                    let frames = buf.frames();
                    if frames == 0 {
                        continue;
                    }
                    self.scratch.resize(frames * ch, 0.0);
                    buf.copy_to_slice_interleaved(&mut self.scratch[..]);
                    (frames, ch)
                }
                Dec::Opus(dec) => {
                    self.scratch.clear();
                    let Ok(n) = dec.decode(&packet.data, &mut self.scratch) else { continue };
                    let ch = dec.channels();
                    // 컨테이너가 알려 준 앞(pre-skip)과 끝(패딩)을 잘라 원래 길이로.
                    let start = (packet.trim_start.get() as usize).min(n);
                    // 컨테이너가 앞을 이미 잘라 준다면 따로 버리지 않는다(두 번 자르지 않게).
                    if std::mem::take(&mut self.first_packet) && start > 0 {
                        self.skip = self.skip.saturating_sub(start as u64);
                    }
                    let end = (packet.trim_end.get() as usize).min(n - start);
                    let frames = n - start - end;
                    if frames == 0 {
                        continue;
                    }
                    self.scratch.truncate((start + frames) * ch);
                    self.scratch.drain(..start * ch);
                    (frames, ch)
                }
            };
            let skip = (self.skip as usize).min(frames);
            self.skip -= skip as u64;
            if skip == frames {
                continue;
            }
            let want = self.spec.channels;
            if ch == want {
                out.extend_from_slice(&self.scratch[skip * ch..]);
            } else {
                // 중간에 채널 수가 바뀌는 드문 파일: 앞 채널을 맞춰 넣는다.
                for f in skip..frames {
                    for c in 0..want {
                        out.push(self.scratch[f * ch + c.min(ch - 1)]);
                    }
                }
            }
            return Ok(frames - skip);
        }
    }

    fn seek(&mut self, frame: u64) -> anyhow::Result<u64> {
        self.first_packet = false;
        let rate = self.spec.rate;
        let time_base = self.format.tracks().iter().find(|t| t.id == self.track_id).and_then(|t| t.time_base);
        // 타임스탬프 단위가 1/샘플레이트면 프레임 번호를 그대로 쓴다. 초로 바꾸면 반올림 때문에 한 프레임 어긋난다.
        let exact = time_base.is_some_and(|tb| tb.numer.get() == 1 && tb.denom.get() == rate);
        // 샘플 단위 시간축(Ogg)은 pre-skip을 포함하고, 시간 단위(WebM)는 이미 뺀 값이다.
        let shift = if exact { self.delay } else { 0 };
        let target = frame + shift;
        // Opus는 앞 패킷들로 상태(에너지 예측)를 쌓으므로, 조금 앞에서부터 풀어 버린다(RFC 7845 4.6은
        // 80ms 이상). 넉넉히 잡아 시크한 자리부터 제 소리가 난다.
        let from = match self.decoder {
            Dec::Opus(_) => target.saturating_sub(OPUS_PREROLL),
            Dec::Sym(_) => target,
        };
        let to = if exact {
            SeekTo::Timestamp { ts: symphonia::core::units::Timestamp::new(from as i64), track_id: self.track_id }
        } else {
            let time = Time::try_from_secs_f64(from as f64 / rate as f64).ok_or_else(|| anyhow!("시간이 범위를 벗어났습니다"))?;
            SeekTo::Time { time, track_id: Some(self.track_id) }
        };
        let seeked = self.format.seek(SeekMode::Accurate, to).map_err(|e| anyhow!("시크 실패: {e}"))?;
        self.decoder.reset();
        let actual = match time_base {
            Some(tb) if !exact => {
                let t = tb.calc_time_saturating(seeked.actual_ts);
                (t.as_secs_f64() * rate as f64).round().max(0.0) as u64
            }
            _ => seeked.actual_ts.get().max(0) as u64,
        };
        // 패킷 경계에서 시작하므로 원하는 프레임까지 버린다. 시간 단위 시간축에서 맨 앞으로 돌아갔으면
        // 첫 패킷의 pre-skip도 버린다.
        self.skip = target.saturating_sub(actual) + if !exact && actual == 0 { self.delay } else { 0 };
        Ok(frame.max(actual.saturating_sub(shift)))
    }
}

/// DSD(1비트, 2.8MHz 이상) → PCM 176.4kHz.
///
/// 1단: 한 바이트(8샘플)마다 표를 찾아 더하는 FIR로 1/8. 비트 하나하나 곱하지 않는다.
/// 2단: 352.8kHz가 될 때까지 반으로 줄이고, 마지막에 30kHz까지 평탄하고 60kHz부터
///      120dB 깎는 날카로운 필터로 반을 더 줄인다. 초음파 대역의 DSD 잡음이 여기서 사라진다.
/// 레벨: SACD 기준(50% 변조 = 0dB)을 0dBFS에 맞춘다(+6dB). 64비트 안에서는 1을 넘어도 괜찮고,
///      헤드룸과 클리핑 표시기가 뒤에서 지킨다.
pub struct DsdSource {
    file: BufReader<File>,
    info: DsdInfo,
    spec: Spec,
    /// 바이트 위치(채널당)
    byte_pos: u64,
    bytes_per_channel: u64,
    tables: Vec<[f64; 256]>,
    hist: Vec<Vec<u8>>,
    hist_pos: usize,
    stages: Vec<Decimator>,
    /// 시크 뒤 필터가 데워질 때까지 버릴 출력 프레임.
    discard: u64,
    raw: Vec<u8>,
    stage1: Vec<f64>,
    tmp: Vec<f64>,
    bit_rev: [u8; 256],
    /// 처음에 버릴 출력 프레임(필터 지연).
    start_discard: u64,
    /// 시크할 때 필터를 데우려고 앞에서부터 풀 출력 프레임 수.
    preroll: u64,
}

pub const DSD_PCM_RATE: u32 = 176_400;
const DSD_GAIN: f64 = 2.0;

impl DsdSource {
    pub fn open(path: &Path) -> anyhow::Result<Self> {
        let info = dsd::probe(path)?;
        if info.rate < 2_822_400 || info.rate % DSD_PCM_RATE != 0 || !(info.rate / DSD_PCM_RATE).is_power_of_two() {
            bail!("지원하지 않는 DSD 샘플레이트: {}", info.rate);
        }
        let channels = info.channels as usize;
        let fs = info.rate as f64;
        let dec = (info.rate / DSD_PCM_RATE) as usize;

        // 2단: 352.8kHz까지 반씩, 마지막에 날카롭게. 각 단의 지연을 1비트 샘플로 센다.
        let mut stages = Vec::new();
        let mut delay = 0usize;
        let mut span = 0usize;
        let mut rate = fs / 8.0;
        loop {
            let last = rate <= 352_800.0 + 1.0;
            let (n, cutoff, gain) = if last {
                (kaiser_taps(120.0, 30_000.0, rate), 45_000.0, DSD_GAIN)
            } else {
                let stop = rate / 2.0 - 60_000.0;
                (kaiser_taps(120.0, stop - 30_000.0, rate), (30_000.0 + stop) / 2.0, 1.0)
            };
            // kaiser_taps는 홀수를 주므로 각 단의 지연은 정수다.
            let period = (fs / rate).round() as usize;
            stages.push(Decimator::new(lowpass(n, cutoff, rate, kaiser_beta(120.0), gain), 2, channels));
            delay += (n - 1) / 2 * period;
            span += n * period;
            rate /= 2.0;
            if last {
                break;
            }
        }

        // 1단: 30kHz까지 평탄, (fs/8 - 60k)부터 120dB. 홀수 길이라 지연이 정수다.
        let stop1 = fs / 8.0 - 60_000.0;
        let n1 = kaiser_taps(120.0, stop1 - 30_000.0, fs);
        let core = lowpass(n1, (30_000.0 + stop1) / 2.0, fs, kaiser_beta(120.0), 1.0);
        // 출력 m번이 정확히 1비트 샘플 dec·m에 오도록, 앞에 0을 z개 붙여 전체 지연을 맞춘다:
        //   (n1-1)/2 + z + delay - (dec-1) ≡ 0 (mod dec)
        let base = (n1 - 1) / 2 + delay;
        let z = (dec - (base + dec - (dec - 1)) % dec) % dec;
        let mut h1 = vec![0.0; z];
        h1.extend_from_slice(&core);
        h1.resize(h1.len().div_ceil(8) * 8, 0.0);
        let total_delay = base + z;
        debug_assert_eq!((total_delay + 1) % dec, 0);
        let start_discard = ((total_delay + 1 - dec) / dec) as u64;
        span += h1.len();

        let nbytes = h1.len() / 8;
        let mut tables = vec![[0.0f64; 256]; nbytes];
        for (j, table) in tables.iter_mut().enumerate() {
            for (b, slot) in table.iter_mut().enumerate() {
                // 바이트의 MSB가 가장 먼저 온 샘플, bit0이 가장 최근 샘플.
                *slot = (0..8).map(|i| if b >> i & 1 == 1 { h1[8 * j + i] } else { -h1[8 * j + i] }).sum();
            }
        }

        let mut bit_rev = [0u8; 256];
        for (i, r) in bit_rev.iter_mut().enumerate() {
            *r = (i as u8).reverse_bits();
        }
        let bytes_per_channel = info.samples.div_ceil(8);
        let out_frames = info.samples / dec as u64;
        let spec = Spec { rate: DSD_PCM_RATE, channels, frames: Some(out_frames), bits: None, dsd_rate: Some(info.rate) };
        let mut file = BufReader::with_capacity(1 << 16, File::open(path)?);
        file.seek(SeekFrom::Start(info.data_offset))?;
        Ok(DsdSource {
            file,
            info,
            spec,
            byte_pos: 0,
            bytes_per_channel,
            tables,
            hist: vec![vec![0x69; nbytes * 2]; channels],
            hist_pos: 0,
            stages,
            discard: start_discard,
            raw: Vec::new(),
            stage1: Vec::new(),
            tmp: Vec::new(),
            bit_rev,
            start_discard,
            preroll: (span / dec + 4) as u64,
        })
    }

    /// 다음 바이트 묶음을 읽어 채널별 바이트 열로 돌려준다.
    fn read_bytes(&mut self, max_per_channel: usize) -> anyhow::Result<Vec<Vec<u8>>> {
        let ch = self.info.channels as usize;
        let left = self.bytes_per_channel.saturating_sub(self.byte_pos) as usize;
        let n = left.min(max_per_channel);
        let mut per = vec![Vec::with_capacity(n); ch];
        if n == 0 {
            return Ok(per);
        }
        if self.info.block_size > 1 {
            // DSF: 채널마다 block_size 바이트씩 번갈아.
            let bs = self.info.block_size as u64;
            let mut done = 0usize;
            while done < n {
                let pos = self.byte_pos + done as u64;
                let block = pos / bs;
                let within = pos % bs;
                let take = ((bs - within) as usize).min(n - done);
                for (c, dst) in per.iter_mut().enumerate() {
                    let off = self.info.data_offset + block * bs * ch as u64 + c as u64 * bs + within;
                    self.file.seek(SeekFrom::Start(off))?;
                    self.raw.resize(take, 0);
                    self.file.read_exact(&mut self.raw[..take])?;
                    dst.extend(self.raw[..take].iter().map(|&b| self.bit_rev[b as usize]));
                }
                done += take;
            }
        } else {
            // DSDIFF: 1바이트씩 번갈아, MSB가 먼저.
            self.raw.resize(n * ch, 0);
            self.file.seek(SeekFrom::Start(self.info.data_offset + self.byte_pos * ch as u64))?;
            self.file.read_exact(&mut self.raw[..n * ch])?;
            for f in 0..n {
                for (c, dst) in per.iter_mut().enumerate() {
                    dst.push(self.raw[f * ch + c]);
                }
            }
        }
        self.byte_pos += n as u64;
        Ok(per)
    }
}

impl PcmSource for DsdSource {
    fn spec(&self) -> &Spec {
        &self.spec
    }

    fn read(&mut self, out: &mut Vec<f64>) -> anyhow::Result<usize> {
        let ch = self.info.channels as usize;
        loop {
            let per = self.read_bytes(4096)?;
            let n = per[0].len();
            if n == 0 {
                return Ok(0);
            }
            let nb = self.tables.len();
            self.stage1.clear();
            for i in 0..n {
                let p = self.hist_pos;
                for (c, bytes) in per.iter().enumerate() {
                    let h = &mut self.hist[c];
                    h[p] = bytes[i];
                    h[p + nb] = bytes[i];
                }
                self.hist_pos = (p + 1) % nb;
                for c in 0..ch {
                    // 가장 최근 바이트가 j = 0.
                    let h = &self.hist[c];
                    let newest = p + nb;
                    let mut acc = 0.0;
                    for (j, t) in self.tables.iter().enumerate() {
                        acc += t[h[newest - j] as usize];
                    }
                    self.stage1.push(acc);
                }
            }
            let mut cur = std::mem::take(&mut self.stage1);
            for st in &mut self.stages {
                self.tmp.clear();
                st.process(&cur, &mut self.tmp);
                std::mem::swap(&mut cur, &mut self.tmp);
            }
            let frames = cur.len() / ch;
            let drop = (self.discard as usize).min(frames);
            self.discard -= drop as u64;
            out.extend_from_slice(&cur[drop * ch..]);
            self.stage1 = cur;
            if frames > drop {
                return Ok(frames - drop);
            }
        }
    }

    fn seek(&mut self, frame: u64) -> anyhow::Result<u64> {
        let dec = (self.info.rate / DSD_PCM_RATE) as u64;
        let frame = frame.min(self.spec.frames.unwrap_or(u64::MAX));
        // 출력 한 프레임 = dec/8 바이트. 그 경계에서 다시 시작해야 시간이 맞는다.
        let start_frame = frame.saturating_sub(self.preroll);
        self.byte_pos = (start_frame * dec / 8).min(self.bytes_per_channel);
        for h in &mut self.hist {
            h.fill(0x69);
        }
        self.hist_pos = 0;
        for st in &mut self.stages {
            st.reset();
        }
        self.discard = self.start_discard + (frame - start_frame);
        Ok(frame)
    }
}
