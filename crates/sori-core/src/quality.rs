//! 음질을 말로 옮긴다. 화면의 배지와 신호 경로가 같은 말을 쓰도록 한곳에 둔다.

use crate::model::{AudioFormat, QualityTier};

/// Hi-Res는 16비트/48kHz를 넘는 무손실.
pub fn tier_of(f: &AudioFormat) -> QualityTier {
    if !f.lossless {
        return QualityTier::Lossy;
    }
    if f.bit_depth.unwrap_or(16) > 16 || f.sample_rate.unwrap_or(44_100) > 48_000 { QualityTier::Hires } else { QualityTier::Lossless }
}

/// 44100 → "44.1", 96000 → "96".
pub fn khz(rate: u32) -> String {
    if rate.is_multiple_of(1000) {
        (rate / 1000).to_string()
    } else {
        let s = format!("{:.1}", rate as f64 / 1000.0);
        s.trim_end_matches(".0").to_string()
    }
}

/// "FLAC 24/96", "MP3 320k", "DSD64" 처럼 짧게.
pub fn format_label(f: &AudioFormat) -> String {
    if f.codec == "DSD" {
        return match f.sample_rate {
            Some(r) => format!("DSD{}", (r as f64 / 44_100.0).round() as u32),
            None => "DSD".into(),
        };
    }
    if f.lossless {
        return match (f.bit_depth, f.sample_rate) {
            (Some(b), Some(r)) => format!("{} {}/{}", f.codec, b, khz(r)),
            (None, Some(r)) => format!("{} {}kHz", f.codec, khz(r)),
            (Some(b), None) => format!("{} {}-bit", f.codec, b),
            (None, None) => f.codec.clone(),
        };
    }
    match f.bitrate {
        Some(b) => format!("{} {}k", f.codec, (b as f64 / 1000.0).round() as u32),
        None => f.codec.clone(),
    }
}

fn rank(f: &AudioFormat) -> f64 {
    (if f.lossless { 1e12 } else { 0.0 })
        + f.bit_depth.unwrap_or(0) as f64 * 1e7
        + f.sample_rate.unwrap_or(0) as f64
        + if f.lossless { 0.0 } else { f.bitrate.unwrap_or(0) as f64 / 1000.0 }
}

/// 두 포맷 중 더 좋은 쪽. 앨범 배지에 쓴다.
pub fn better_format<'a>(a: &'a AudioFormat, b: &'a AudioFormat) -> &'a AudioFormat {
    if rank(b) > rank(a) { b } else { a }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::formats::{FormatFacts, to_audio_format};

    fn fmt(codec: &'static str, ext: &str, bits: Option<u32>, rate: Option<u32>, br: Option<u32>) -> AudioFormat {
        to_audio_format(FormatFacts { codec, sample_rate: rate, bit_depth: bits, bitrate: br, channels: Some(2) }, ext)
    }

    #[test]
    fn labels() {
        assert_eq!(format_label(&fmt("FLAC", "flac", Some(24), Some(96_000), None)), "FLAC 24/96");
        assert_eq!(format_label(&fmt("FLAC", "flac", Some(16), Some(44_100), None)), "FLAC 16/44.1");
        assert_eq!(format_label(&fmt("MP3", "mp3", None, Some(44_100), Some(320_000))), "MP3 320k");
        assert_eq!(format_label(&fmt("DSD", "dsf", Some(1), Some(2_822_400), None)), "DSD64");
        assert_eq!(khz(88_200), "88.2");
        assert_eq!(khz(48_000), "48");
    }

    #[test]
    fn tiers() {
        assert_eq!(tier_of(&fmt("FLAC", "flac", Some(24), Some(96_000), None)), QualityTier::Hires);
        assert_eq!(tier_of(&fmt("FLAC", "flac", Some(16), Some(44_100), None)), QualityTier::Lossless);
        assert_eq!(tier_of(&fmt("ALAC", "m4a", Some(24), Some(48_000), None)), QualityTier::Hires);
        assert_eq!(tier_of(&fmt("MP3", "mp3", None, Some(96_000), Some(320_000))), QualityTier::Lossy);
    }

    #[test]
    fn better() {
        let a = fmt("FLAC", "flac", Some(16), Some(44_100), None);
        let b = fmt("FLAC", "flac", Some(24), Some(48_000), None);
        let c = fmt("MP3", "mp3", None, Some(48_000), Some(320_000));
        assert_eq!(better_format(&a, &b), &b);
        assert_eq!(better_format(&c, &a), &a);
    }
}
