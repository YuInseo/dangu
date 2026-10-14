//! 볼륨 레벨링(룬과 같은 규칙).
//! - 목표 라우드니스(기본 -14 LUFS)에서 곡의 라우드니스를 뺀 만큼 게인을 준다.
//! - 올리는 게인은 트루 피크를 넘지 않게 제한한다. 레벨링 때문에 클리핑이 나지 않는다.
//! - 라우드니스를 모르는 곡에는 고정 게인(기본 -5dB)을 준다.

use crate::dsp::request::LevelBasis;
use crate::model::Loudness;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Basis {
    Track,
    Album,
    Unknown,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LevelingResult {
    pub gain: f64,
    pub basis: Basis,
    pub limited: bool,
    pub lufs: Option<f64>,
}

fn round1(x: f64) -> f64 {
    (x * 10.0).round() / 10.0
}

pub fn leveling_gain(l: Option<&Loudness>, mode: LevelBasis, target: f64, unknown: f64) -> LevelingResult {
    let (v, basis) = match mode {
        LevelBasis::Album => match l.and_then(|l| l.album) {
            Some(a) => (Some(a), Basis::Album),
            None => (l.and_then(|l| l.track), Basis::Track),
        },
        LevelBasis::Track => (l.and_then(|l| l.track), Basis::Track),
    };
    // 무음에 가까운 곡(-70 LUFS 게이트)은 모르는 곡으로 친다. 아니면 +50dB를 주려 든다.
    let Some(v) = v.filter(|v| v.lufs.is_finite() && v.lufs > -69.0) else {
        return LevelingResult { gain: round1(unknown), basis: Basis::Unknown, limited: false, lufs: None };
    };
    let mut gain = target - v.lufs;
    let mut limited = false;
    if gain > 0.0 && v.peak.is_finite() {
        let ceiling = -v.peak;
        if gain > ceiling {
            gain = ceiling.max(0.0);
            limited = true;
        }
    }
    LevelingResult { gain: round1(gain), basis, limited, lufs: Some(v.lufs) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{LoudnessSource, LoudnessValue};

    #[test]
    fn rules() {
        let l = Loudness {
            track: Some(LoudnessValue { lufs: -9.0, peak: 0.5 }),
            album: Some(LoudnessValue { lufs: -10.0, peak: 0.8 }),
            source: LoudnessSource::Analysis,
        };
        let r = leveling_gain(Some(&l), LevelBasis::Track, -14.0, -5.0);
        assert_eq!((r.gain, r.basis, r.limited), (-5.0, Basis::Track, false));
        let r = leveling_gain(Some(&l), LevelBasis::Album, -14.0, -5.0);
        assert_eq!(r.gain, -4.0);
        // 조용한 곡은 트루 피크까지만 올린다
        let quiet = Loudness { track: Some(LoudnessValue { lufs: -24.0, peak: -3.0 }), album: None, source: LoudnessSource::Analysis };
        let r = leveling_gain(Some(&quiet), LevelBasis::Track, -14.0, -5.0);
        assert_eq!((r.gain, r.limited), (3.0, true));
        assert_eq!(leveling_gain(None, LevelBasis::Track, -14.0, -5.0).basis, Basis::Unknown);
    }
}
