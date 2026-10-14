//! 네이티브 재생 엔진: 갭리스가 샘플 단위로 맞는지, 시크·볼륨·끝 알림이 맞는지.
mod common;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use sori_core::decode;
use sori_core::dsp::resample::{Quality, Resampler};
use sori_core::player::output::NullOutput;
use sori_core::player::{Event, Opened, PlayItem, PlaySource, Player, Status};

fn tracks(dir: &str) -> Vec<PathBuf> {
    let s = common::samples().unwrap();
    let mut v: Vec<PathBuf> =
        std::fs::read_dir(s.join(dir)).unwrap().flatten().map(|e| e.path()).filter(|p| !p.to_string_lossy().ends_with(".jpg")).collect();
    v.sort();
    v
}

fn item(p: &std::path::Path) -> PlayItem {
    PlayItem {
        uid: p.file_name().unwrap().to_string_lossy().to_string(),
        source: PlaySource::Url { url: p.to_string_lossy().to_string() },
        dsp: None,
    }
}

fn player(rate: u32) -> (Player, Arc<parking_lot::Mutex<Vec<f64>>>) {
    let out = NullOutput::new(rate, false);
    let captured = out.captured.clone();
    let resolver = Arc::new(|it: &PlayItem| -> anyhow::Result<Opened> {
        let PlaySource::Url { url } = &it.source else { anyhow::bail!("주소가 아님") };
        Ok(Opened { source: decode::open(std::path::Path::new(url))?, loudness: None })
    });
    (Player::new(Box::new(move || Box::new(out)), resolver), captured)
}

fn wait_for(p: &Player, what: impl Fn(&[(u64, Event)]) -> bool) {
    let t = Instant::now();
    while t.elapsed() < Duration::from_secs(60) {
        if what(&p.state().events) {
            return;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!("기다리던 일이 오지 않았습니다: {:?}", p.state());
}

fn ended(e: &[(u64, Event)]) -> bool {
    e.iter().any(|(_, e)| matches!(e, Event::Ended { .. }))
}

#[test]
fn gapless_is_sample_exact() {
    if common::samples().is_none() {
        return;
    }
    let t = tracks("Neon Harbor/2023 - Night Drive"); // FLAC 24/96
    let (p, captured) = player(96_000);
    p.load(item(&t[0]), true, 0.0, false);
    p.preload(Some(item(&t[1])));
    wait_for(&p, |e| e.iter().any(|(_, e)| matches!(e, Event::Advanced { uid } if *uid == item(&t[1]).uid)));
    wait_for(&p, ended);
    let (_, a) = decode::decode_all(&t[0]).unwrap();
    let (_, b) = decode::decode_all(&t[1]).unwrap();
    let got = captured.lock().clone();
    assert_eq!(got.len(), a.len() + b.len(), "길이");
    // 원본 그대로(볼륨 1, 레벨링 0dB, 같은 샘플레이트): 비트 단위로 같아야 한다.
    assert!(got[..a.len()] == a[..], "첫 곡");
    assert!(got[a.len()..] == b[..], "둘째 곡이 첫 곡 바로 다음 샘플에 붙어야 한다");
    let st = p.state();
    assert_eq!(st.status, Status::Paused);
    assert_eq!(st.resampling, None);
}

#[test]
fn resampled_gapless_matches_one_long_conversion() {
    if common::samples().is_none() {
        return;
    }
    let t = tracks("Neon Harbor/2023 - Night Drive");
    let (p, captured) = player(48_000);
    p.load(item(&t[0]), true, 0.0, false);
    p.preload(Some(item(&t[1])));
    wait_for(&p, ended);
    let (_, a) = decode::decode_all(&t[0]).unwrap();
    let (_, b) = decode::decode_all(&t[1]).unwrap();
    let mut joined = a.clone();
    joined.extend_from_slice(&b);
    let mut rs = Resampler::new(96_000, 48_000, 2, Quality::default());
    let mut want = Vec::new();
    rs.process(&joined, &mut want);
    rs.flush(&mut want);
    let got = captured.lock().clone();
    assert_eq!(got.len(), want.len(), "길이: 두 곡을 한 번에 변환한 것과 같아야 한다");
    let max = got.iter().zip(&want).map(|(x, y)| (x - y).abs()).fold(0.0, f64::max);
    assert!(max < 1e-12, "이음매에서 어긋남: {max}");
}

#[test]
fn seek_and_volume() {
    if common::samples().is_none() {
        return;
    }
    let t = tracks("하늘 사중주/아침의 차"); // FLAC 16/44.1
    let (p, captured) = player(44_100);
    p.set_volume(0.25, false); // 선형 진폭 → 0.25배(-12dB)
    p.load(item(&t[0]), false, 0.0, false);
    std::thread::sleep(Duration::from_millis(50));
    p.seek(5.0);
    p.play();
    wait_for(&p, ended);
    let (_, a) = decode::decode_all(&t[0]).unwrap();
    let got = captured.lock().clone();
    let from = 5 * 44_100 * 2;
    assert_eq!(got.len(), a.len() - from);
    // 시작 부분은 볼륨이 1 → 0.25로 옮겨 가는 중일 수 있으니 조금 뒤부터 본다.
    let max = got[4096..].iter().zip(&a[from + 4096..]).map(|(x, y)| (x - 0.25 * y).abs()).fold(0.0, f64::max);
    assert!(max < 1e-12, "볼륨/시크 어긋남 {max}");
}

#[test]
fn errors_are_reported() {
    let (p, _) = player(48_000);
    p.load(PlayItem { uid: "x".into(), source: PlaySource::Url { url: "/없는/파일.flac".into() }, dsp: None }, true, 0.0, false);
    wait_for(&p, |e| e.iter().any(|(_, e)| matches!(e, Event::Error { .. })));
    assert_eq!(p.state().status, Status::Error);
}

#[test]
fn dsp_chain_flows_across_the_gapless_join() {
    use sori_core::dsp::chain::Chain;
    use sori_core::dsp::request::{Band, BandType, Dither, DspRequest, Filter, Out, encode_request};
    if common::samples().is_none() {
        return;
    }
    let t = tracks("Neon Harbor/2023 - Night Drive"); // 96kHz → 장치 96kHz(변환 없음)
    let req = DspRequest {
        v: 1,
        hr: Some(-3.0),
        ci: Some(1),
        lv: None,
        src: None,
        out: Out { b: 24, d: Dither::Tpdf },
        f: vec![Filter::Peq { pre: -1.0, b: vec![Band { kind: BandType::Peak, freq: 2500.0, gain: 4.0, q: 1.4, on: true }] }],
    };
    let enc = encode_request(&req);
    let (p, captured) = player(96_000);
    let mut a = item(&t[0]);
    a.dsp = Some(enc.clone());
    let mut b = item(&t[1]);
    b.dsp = Some(enc.clone());
    p.load(a, true, 0.0, false);
    p.preload(Some(b));
    wait_for(&p, ended);
    let (_, x) = decode::decode_all(&t[0]).unwrap();
    let (_, y) = decode::decode_all(&t[1]).unwrap();
    let mut joined = x.clone();
    joined.extend_from_slice(&y);
    let mut chain = Chain::new(&req, 96_000, None);
    chain.process(&mut joined);
    let got = captured.lock().clone();
    assert_eq!(got.len(), joined.len());
    let max = got.iter().zip(&joined).map(|(a, b)| (a - b).abs()).fold(0.0, f64::max);
    assert!(max < 1e-12, "체인이 곡 사이에서 끊겼다: {max}");
}

#[test]
fn changing_dsp_mid_track_applies_from_the_playing_position() {
    use sori_core::dsp::request::{Dither, DspRequest, Out, encode_request};
    if common::samples().is_none() {
        return;
    }
    let t = tracks("하늘 사중주/아침의 차"); // 44.1kHz
    let (p, captured) = player(44_100);
    p.load(item(&t[0]), false, 0.0, false);
    std::thread::sleep(Duration::from_millis(50));
    p.seek(10.0);
    let req = DspRequest { v: 1, hr: Some(-6.0), ci: None, lv: None, src: None, out: Out { b: 24, d: Dither::Tpdf }, f: vec![] };
    p.set_dsp(vec![(item(&t[0]).uid, Some(encode_request(&req)))]);
    std::thread::sleep(Duration::from_millis(50));
    p.play();
    wait_for(&p, ended);
    let (_, a) = decode::decode_all(&t[0]).unwrap();
    let got = captured.lock().clone();
    let from = 10 * 44_100 * 2;
    assert_eq!(got.len(), a.len() - from);
    let g = 10f64.powf(-6.0 / 20.0);
    let max = got.iter().zip(&a[from..]).map(|(x, y)| (x - g * y).abs()).fold(0.0, f64::max);
    assert!(max < 1e-12, "{max}");
}
