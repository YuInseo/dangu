//! 파일 확장자와 태그 파서가 알려준 코덱을 우리 `AudioFormat`으로.

use crate::model::AudioFormat;

pub const AUDIO_EXTENSIONS: &[&str] = &[
    "flac", "mp3", "m4a", "mp4", "aac", "wav", "aif", "aiff", "aifc", "ogg", "oga", "opus", "wv", "ape", "dsf", "dff", "webm", "weba",
    "mka",
];

/// 태그 읽는 쪽(lofty)이 모르는 컨테이너. 길이와 형식은 디코더 쪽(symphonia)으로 읽는다.
pub fn needs_container_probe(ext: &str) -> bool {
    matches!(ext, "webm" | "weba" | "mka")
}

pub fn is_audio_ext(ext: &str) -> bool {
    AUDIO_EXTENSIONS.contains(&ext)
}

const LOSSLESS: &[&str] = &["FLAC", "ALAC", "WAV", "AIFF", "PCM", "WavPack", "APE", "DSD"];

pub fn is_lossless_codec(codec: &str) -> bool {
    LOSSLESS.contains(&codec)
}

/// 브라우저에 `canPlayType`으로 물어볼 MIME.
pub fn mime_for(codec: &str, ext: &str) -> &'static str {
    match codec {
        "FLAC" if ext == "ogg" || ext == "oga" => "audio/ogg; codecs=\"flac\"",
        "FLAC" => "audio/flac",
        "MP3" => "audio/mpeg",
        "AAC" if ext == "aac" => "audio/aac",
        "AAC" => "audio/mp4; codecs=\"mp4a.40.2\"",
        "E-AC-3" => "audio/mp4; codecs=\"ec-3\"",
        "AC-3" => "audio/mp4; codecs=\"ac-3\"",
        "AC-4" => "audio/mp4; codecs=\"ac-4\"",
        "ALAC" => "audio/mp4; codecs=\"alac\"",
        "WAV" => "audio/wav",
        "AIFF" => "audio/aiff",
        "Opus" | "Vorbis" if ext == "webm" || ext == "weba" => {
            if codec == "Opus" {
                "audio/webm; codecs=\"opus\""
            } else {
                "audio/webm; codecs=\"vorbis\""
            }
        }
        "Vorbis" => "audio/ogg; codecs=\"vorbis\"",
        "Opus" => "audio/ogg; codecs=\"opus\"",
        "WavPack" => "audio/x-wavpack",
        "APE" => "audio/x-ape",
        "DSD" => "audio/x-dsf",
        _ => "application/octet-stream",
    }
}

/// `audio/mp4; codecs="alac"` → `audio/mp4`. 응답 헤더에는 파라미터를 붙이지 않는다.
pub fn base_mime(mime: &str) -> &str {
    mime.split(';').next().unwrap_or(mime).trim()
}

pub struct FormatFacts {
    pub codec: &'static str,
    pub sample_rate: Option<u32>,
    pub bit_depth: Option<u32>,
    pub bitrate: Option<u32>,
    pub channels: Option<u32>,
}

pub fn to_audio_format(f: FormatFacts, ext: &str) -> AudioFormat {
    let lossless = is_lossless_codec(f.codec);
    AudioFormat {
        codec: f.codec.to_string(),
        container: ext.to_string(),
        lossless,
        sample_rate: f.sample_rate.filter(|&r| r > 0),
        // DSD의 1비트는 PCM 비트 깊이와 비교할 수 없어 적지 않는다.
        bit_depth: if f.codec == "DSD" { None } else { f.bit_depth.filter(|&b| b > 0) },
        bitrate: f.bitrate.filter(|&b| b > 0),
        channels: f.channels.filter(|&c| c > 0),
        mime: mime_for(f.codec, ext).to_string(),
    }
}
