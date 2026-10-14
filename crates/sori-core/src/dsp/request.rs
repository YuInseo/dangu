//! 화면이 보낸 DSP 요청. 화면의 `buildRequest`/`encodeRequest`와 짝이다(base64url JSON).
//!
//! 서버는 받은 것을 그대로 믿지 않는다. 숫자는 전부 범위 안으로 자르고, 이름은 정해진
//! 목록에서만 고른다. 화면의 `parseRequest`와 같은 규칙이다.

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const STANDARD_RATES: [u32; 8] = [44_100, 48_000, 88_200, 96_000, 176_400, 192_000, 352_800, 384_000];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BandType {
    Peak,
    Lowshelf,
    Highshelf,
    Lowpass,
    Highpass,
    Bandpass,
    Notch,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Band {
    #[serde(rename = "type")]
    pub kind: BandType,
    pub freq: f64,
    pub gain: f64,
    pub q: f64,
    pub on: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Channel {
    L,
    R,
    LR,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "lowercase")]
pub enum Step {
    Volume { ch: Channel, gain: f64 },
    Delay { ch: Channel, ms: f64 },
    Invert { ch: Channel },
    Mute { ch: Channel },
    Mix { ll: f64, lr: f64, rl: f64, rr: f64 },
    Eq { ch: Channel, band: Band },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CrossfeedPreset {
    Default,
    Cmoy,
    Jmeier,
    Custom,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "t", rename_all = "lowercase")]
pub enum Filter {
    Crossfeed {
        p: CrossfeedPreset,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        fc: Option<f64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        fd: Option<f64>,
    },
    Peq {
        pre: f64,
        b: Vec<Band>,
    },
    Procedural {
        s: Vec<Step>,
    },
    Headphone {
        pre: f64,
        b: Vec<Band>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        m: Option<String>,
    },
    Convolution {
        ir: String,
        g: f64,
        n: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        name: Option<String>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LevelBasis {
    Track,
    Album,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Leveling {
    pub m: LevelBasis,
    /// 목표 LUFS
    pub t: f64,
    /// 라우드니스를 모를 때 게인
    pub u: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SrcFilter {
    Precise,
    Smooth,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Src {
    pub r: u32,
    pub f: SrcFilter,
    pub p: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Dither {
    Tpdf,
    None,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Out {
    pub b: u32,
    pub d: Dither,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DspRequest {
    pub v: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hr: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ci: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lv: Option<Leveling>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub src: Option<Src>,
    pub out: Out,
    pub f: Vec<Filter>,
}

impl DspRequest {
    pub fn clip_indicator(&self) -> bool {
        self.ci.is_some()
    }
}

fn num(v: Option<&Value>, lo: f64, hi: f64, fallback: f64) -> f64 {
    let n = match v {
        Some(Value::Number(n)) => n.as_f64(),
        Some(Value::String(s)) => s.trim().parse::<f64>().ok(),
        _ => None,
    };
    match n {
        Some(n) if n.is_finite() => n.clamp(lo, hi),
        _ => fallback,
    }
}

fn s(v: Option<&Value>) -> Option<&str> {
    v.and_then(Value::as_str)
}

fn band(v: &Value) -> Option<Band> {
    let o = v.as_object()?;
    let kind = match s(o.get("type")) {
        Some("lowshelf") => BandType::Lowshelf,
        Some("highshelf") => BandType::Highshelf,
        Some("lowpass") => BandType::Lowpass,
        Some("highpass") => BandType::Highpass,
        Some("bandpass") => BandType::Bandpass,
        Some("notch") => BandType::Notch,
        _ => BandType::Peak,
    };
    Some(Band {
        kind,
        freq: num(o.get("freq"), 10.0, 40_000.0, 1000.0),
        gain: num(o.get("gain"), -30.0, 30.0, 0.0),
        q: num(o.get("q"), 0.05, 30.0, 1.0),
        on: o.get("on") != Some(&Value::Bool(false)),
    })
}

fn bands(v: Option<&Value>) -> Vec<Band> {
    v.and_then(Value::as_array).map(|a| a.iter().take(32).filter_map(band).collect()).unwrap_or_default()
}

fn channel(v: Option<&Value>) -> Channel {
    match s(v) {
        Some("L") => Channel::L,
        Some("R") => Channel::R,
        _ => Channel::LR,
    }
}

fn step(v: &Value) -> Option<Step> {
    let o = v.as_object()?;
    let ch = channel(o.get("ch"));
    Some(match s(o.get("op"))? {
        "volume" => Step::Volume { ch, gain: num(o.get("gain"), -60.0, 24.0, 0.0) },
        "delay" => Step::Delay { ch, ms: num(o.get("ms"), 0.0, 500.0, 0.0) },
        "invert" => Step::Invert { ch },
        "mute" => Step::Mute { ch },
        "mix" => Step::Mix {
            ll: num(o.get("ll"), -4.0, 4.0, 1.0),
            lr: num(o.get("lr"), -4.0, 4.0, 0.0),
            rl: num(o.get("rl"), -4.0, 4.0, 0.0),
            rr: num(o.get("rr"), -4.0, 4.0, 1.0),
        },
        "eq" => Step::Eq { ch, band: band(o.get("band")?)? },
        _ => return None,
    })
}

pub fn valid_ir_id(id: &str) -> bool {
    (6..=40).contains(&id.len()) && id.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
}

fn short(v: Option<&Value>, max: usize) -> Option<String> {
    s(v).map(|x| x.chars().take(max).collect())
}

fn filter(v: &Value) -> Option<Filter> {
    let o = v.as_object()?;
    Some(match s(o.get("t"))? {
        "crossfeed" => {
            let p = match s(o.get("p")) {
                Some("cmoy") => CrossfeedPreset::Cmoy,
                Some("jmeier") => CrossfeedPreset::Jmeier,
                Some("custom") => CrossfeedPreset::Custom,
                _ => CrossfeedPreset::Default,
            };
            if p == CrossfeedPreset::Custom {
                Filter::Crossfeed {
                    p,
                    fc: Some(num(o.get("fc"), 300.0, 2000.0, 700.0).round()),
                    fd: Some(num(o.get("fd"), 1.0, 15.0, 4.5)),
                }
            } else {
                Filter::Crossfeed { p, fc: None, fd: None }
            }
        }
        "peq" => Filter::Peq { pre: num(o.get("pre"), -30.0, 12.0, 0.0), b: bands(o.get("b")) },
        "headphone" => Filter::Headphone { pre: num(o.get("pre"), -30.0, 12.0, 0.0), b: bands(o.get("b")), m: short(o.get("m"), 80) },
        "procedural" => Filter::Procedural {
            s: o.get("s").and_then(Value::as_array).map(|a| a.iter().take(32).filter_map(step).collect()).unwrap_or_default(),
        },
        "convolution" => {
            let ir = s(o.get("ir"))?;
            if !valid_ir_id(ir) {
                return None;
            }
            Filter::Convolution {
                ir: ir.to_string(),
                g: num(o.get("g"), -30.0, 12.0, 0.0),
                n: o.get("n") != Some(&Value::Bool(false)),
                name: short(o.get("name"), 120),
            }
        }
        _ => return None,
    })
}

/// 주소에 담겨 온 문자열 → 안전한 요청. 이상하면 None.
pub fn parse_request(encoded: &str) -> Option<DspRequest> {
    if encoded.is_empty() || encoded.len() > 16_000 {
        return None;
    }
    let bytes = URL_SAFE_NO_PAD.decode(encoded.trim_end_matches('=')).ok()?;
    let raw: Value = serde_json::from_slice(&bytes).ok()?;
    parse_value(&raw)
}

pub fn parse_value(raw: &Value) -> Option<DspRequest> {
    let o = raw.as_object()?;
    if o.get("v").and_then(Value::as_u64) != Some(1) {
        return None;
    }
    let out = o.get("out").and_then(Value::as_object);
    let out = Out {
        b: if out.and_then(|x| x.get("b")).and_then(Value::as_u64) == Some(16) { 16 } else { 24 },
        d: if out.and_then(|x| s(x.get("d"))) == Some("none") { Dither::None } else { Dither::Tpdf },
    };
    let f = o.get("f").and_then(Value::as_array).map(|a| a.iter().take(16).filter_map(filter).collect()).unwrap_or_default();
    let mut req = DspRequest { v: 1, hr: None, ci: None, lv: None, src: None, out, f };
    if o.get("hr").is_some_and(|v| !v.is_null()) {
        req.hr = Some(num(o.get("hr"), -30.0, 0.0, 0.0));
    }
    if o.get("ci").is_some_and(|v| v.as_bool() == Some(true) || v.as_u64().is_some_and(|n| n != 0)) {
        req.ci = Some(1);
    }
    if let Some(lv) = o.get("lv").and_then(Value::as_object) {
        req.lv = Some(Leveling {
            m: if s(lv.get("m")) == Some("album") { LevelBasis::Album } else { LevelBasis::Track },
            t: num(lv.get("t"), -30.0, -5.0, -14.0),
            u: num(lv.get("u"), -20.0, 6.0, -5.0),
        });
    }
    if let Some(src) = o.get("src").and_then(Value::as_object) {
        let r = src.get("r").and_then(Value::as_u64).unwrap_or(0) as u32;
        if STANDARD_RATES.contains(&r) {
            let p = src.get("p").and_then(Value::as_u64).unwrap_or(28) as u32;
            req.src = Some(Src {
                r,
                f: if s(src.get("f")) == Some("smooth") { SrcFilter::Smooth } else { SrcFilter::Precise },
                p: if [20, 28, 33].contains(&p) { p } else { 28 },
            });
        }
    }
    Some(req)
}

pub fn encode_request(req: &DspRequest) -> String {
    URL_SAFE_NO_PAD.encode(serde_json::to_vec(req).unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn clamps_and_drops() {
        let raw = json!({
            "v": 1, "hr": -99, "ci": 1,
            "lv": { "m": "album", "t": -2, "u": -50 },
            "src": { "r": 12345, "f": "smooth", "p": 33 },
            "out": { "b": 16, "d": "none" },
            "f": [
                { "t": "peq", "pre": 50, "b": [{ "type": "peak", "freq": 99999, "gain": -99, "q": 0, "on": true }, "쓰레기"] },
                { "t": "convolution", "ir": "../../etc/passwd", "g": 0, "n": true },
                { "t": "crossfeed", "p": "custom", "fc": 10, "fd": 99 },
                { "t": "procedural", "s": [{ "op": "delay", "ch": "L", "ms": 9999 }, { "op": "rm -rf" }] },
                { "t": "????" }
            ]
        });
        let r = parse_value(&raw).unwrap();
        assert_eq!(r.hr, Some(-30.0));
        assert_eq!(r.lv, Some(Leveling { m: LevelBasis::Album, t: -5.0, u: -20.0 }));
        assert_eq!(r.src, None, "표준이 아닌 샘플레이트는 버린다");
        assert_eq!(r.out, Out { b: 16, d: Dither::None });
        assert_eq!(r.f.len(), 3, "IR ID가 이상한 컨볼루션과 모르는 필터는 빠진다");
        match &r.f[0] {
            Filter::Peq { pre, b } => {
                assert_eq!(*pre, 12.0);
                assert_eq!(b.len(), 1);
                assert_eq!((b[0].freq, b[0].gain, b[0].q), (40_000.0, -30.0, 0.05));
            }
            _ => panic!(),
        }
        assert_eq!(r.f[1], Filter::Crossfeed { p: CrossfeedPreset::Custom, fc: Some(300.0), fd: Some(15.0) });
        assert_eq!(r.f[2], Filter::Procedural { s: vec![Step::Delay { ch: Channel::L, ms: 500.0 }] });
    }

    #[test]
    fn round_trip() {
        let raw = json!({ "v": 1, "out": { "b": 24, "d": "tpdf" }, "f": [{ "t": "crossfeed", "p": "jmeier" }], "src": { "r": 96000, "f": "precise", "p": 28 } });
        let r = parse_value(&raw).unwrap();
        let back = parse_request(&encode_request(&r)).unwrap();
        assert_eq!(r, back);
        assert!(parse_request("not base64 !!!").is_none());
        assert!(parse_value(&json!({ "v": 2, "out": {}, "f": [] })).is_none());
    }
}
