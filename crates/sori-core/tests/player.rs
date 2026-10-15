//! 네이티브 재생 엔진: 갭리스가 샘플 단위로 맞는지, 시크·볼륨·끝 알림이 맞는지.
mod common;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use sori_core::decode::{self, PcmSource, Spec};
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

/// 시험에서는 라이브러리 곡의 ID 자리에 파일 경로를 쓴다.
fn item(p: &std::path::Path) -> PlayItem {
    PlayItem {
        uid: p.file_name().unwrap().to_string_lossy().to_string(),
        source: PlaySource::Library { id: p.to_string_lossy().to_string() },
        dsp: None,
        meta: None,
    }
}

/// 네트워크 곡 흉내: 여는 데 `open`만큼, 시크하는 데 `seek`만큼 걸린다.
fn remote_item(p: &std::path::Path) -> PlayItem {
    PlayItem {
        uid: format!("live:{}", p.file_name().unwrap().to_string_lossy()),
        source: PlaySource::Live { id: p.to_string_lossy().to_string(), file: String::new() },
        dsp: None,
        meta: None,
    }
}

struct Slow {
    inner: Box<dyn PcmSource>,
    seek: Duration,
}

impl PcmSource for Slow {
    fn spec(&self) -> &Spec {
        self.inner.spec()
    }
    fn read(&mut self, out: &mut Vec<f64>) -> anyhow::Result<usize> {
        self.inner.read(out)
    }
    fn seek(&mut self, frame: u64) -> anyhow::Result<u64> {
        std::thread::sleep(self.seek);
        self.inner.seek(frame)
    }
    fn remote(&self) -> bool {
        true
    }
}

fn player_with(rate: u32, realtime: bool, open: Duration, seek: Duration) -> (Player, Arc<parking_lot::Mutex<Vec<f64>>>) {
    let out = NullOutput::new(rate, realtime);
    let captured = out.captured.clone();
    let resolver = Arc::new(move |it: &PlayItem| -> anyhow::Result<Opened> {
        match &it.source {
            PlaySource::Library { id } => Ok(Opened { source: decode::open(std::path::Path::new(id))?, loudness: None }),
            PlaySource::Live { id, .. } => {
                std::thread::sleep(open);
                Ok(Opened { source: Box::new(Slow { inner: decode::open(std::path::Path::new(id))?, seek }), loudness: None })
            }
        }
    });
    (Player::new(Box::new(move || Box::new(out)), resolver), captured)
}

fn player(rate: u32) -> (Player, Arc<parking_lot::Mutex<Vec<f64>>>) {
    player_with(rate, false, Duration::ZERO, Duration::ZERO)
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
    p.load(
        PlayItem { uid: "x".into(), source: PlaySource::Library { id: "/없는/파일.flac".into() }, dsp: None, meta: None },
        true,
        0.0,
        false,
    );
    wait_for(&p, |e| e.iter().any(|(_, e)| matches!(e, Event::Error { .. })));
    assert_eq!(p.state().status, Status::Error);
    // 네트워크 곡도 뒤에서 열다 실패하면 같은 길로 알린다.
    let (p, _) = player_with(48_000, false, Duration::from_millis(50), Duration::ZERO);
    p.load(remote_item(std::path::Path::new("/없는/곡.flac")), true, 0.0, false);
    wait_for(&p, |e| e.iter().any(|(_, e)| matches!(e, Event::Error { uid: Some(u), .. } if u == "live:곡.flac")));
    assert_eq!(p.state().status, Status::Error);
}

#[test]
fn slow_network_tracks_are_still_gapless() {
    if common::samples().is_none() {
        return;
    }
    let t = tracks("Neon Harbor/2023 - Night Drive"); // FLAC 24/96
    // 여는 데 0.4초: 첫 곡이 다 끝나도 다음 곡이 아직 안 열렸을 수 있다(출력은 시간을 기다리지 않는다).
    let (p, captured) = player_with(96_000, false, Duration::from_millis(400), Duration::ZERO);
    let (a, b) = (remote_item(&t[0]), remote_item(&t[1]));
    p.load(a.clone(), true, 0.0, false);
    p.preload(Some(b.clone()));
    // 여는 동안에도 화면은 이 곡을 보여 주고, 명령은 바로 듣는다.
    std::thread::sleep(Duration::from_millis(30));
    let st = p.state();
    assert_eq!(st.uid.as_deref(), Some(a.uid.as_str()));
    assert_eq!(st.status, Status::Loading);
    wait_for(&p, |e| e.iter().any(|(_, e)| matches!(e, Event::Advanced { uid } if *uid == b.uid)));
    wait_for(&p, ended);
    let (_, x) = decode::decode_all(&t[0]).unwrap();
    let (_, y) = decode::decode_all(&t[1]).unwrap();
    let got = captured.lock().clone();
    assert_eq!(got.len(), x.len() + y.len(), "길이");
    assert!(got[..x.len()] == x[..], "첫 곡");
    assert!(got[x.len()..] == y[..], "다음 곡을 늦게 열었어도 첫 곡 바로 다음 샘플에 붙어야 한다");
}

#[test]
fn controls_answer_while_a_track_opens_or_seeks() {
    if common::samples().is_none() {
        return;
    }
    let t = tracks("하늘 사중주/아침의 차"); // FLAC 16/44.1
    let (p, captured) = player_with(44_100, false, Duration::from_millis(600), Duration::from_millis(600));
    let a = remote_item(&t[0]);
    p.load(a.clone(), false, 0.0, false);
    let quick = |p: &Player, what: &dyn Fn(&sori_core::player::PlayerState) -> bool| {
        let t0 = Instant::now();
        while t0.elapsed() < Duration::from_millis(200) {
            if what(&p.state()) {
                return;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        panic!("여는 동안 명령이 막혔습니다: {:?}", p.state());
    };
    p.set_volume(0.5, false);
    quick(&p, &|s| s.volume == 0.5);
    p.set_volume(1.0, false);
    quick(&p, &|s| s.volume == 1.0);
    // 여는 중에 시크하면 열린 뒤 그 위치로 간다.
    p.seek(3.0);
    quick(&p, &|s| s.position == 3.0);
    wait_for(&p, |_| p.state().status == Status::Paused && p.state().source.is_some());
    // 시크(0.6초 걸린다)하는 동안에도 명령을 듣는다.
    p.seek(5.0);
    p.set_volume(0.75, false);
    quick(&p, &|s| s.volume == 0.75);
    p.set_volume(1.0, false);
    std::thread::sleep(Duration::from_millis(50));
    p.play();
    wait_for(&p, ended);
    let (_, x) = decode::decode_all(&t[0]).unwrap();
    let got = captured.lock().clone();
    let from = 5 * 44_100 * 2;
    assert_eq!(got.len(), x.len() - from, "5초부터 끝까지");
    assert!(got[4096..] == x[from + 4096..], "시크한 위치부터 그대로");
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

fn advanced_to(e: &[(u64, Event)], uid: &str) -> bool {
    e.iter().any(|(_, e)| matches!(e, Event::Advanced { uid: u } if u == uid))
}

#[test]
fn engine_queue_plays_on_without_the_screen() {
    if common::samples().is_none() {
        return;
    }
    // 화면이 잠들어도(대기열을 더 보내지 않아도) 엔진이 들고 있는 다음 곡들을 이어 튼다.
    let t = tracks("Neon Harbor/2023 - Night Drive");
    let (p, captured) = player(96_000);
    p.load(item(&t[0]), true, 0.0, false);
    p.set_queue(vec![item(&t[1]), item(&t[2])], vec![], false);
    wait_for(&p, ended);
    let mut want = Vec::new();
    for f in &t[..3] {
        want.extend(decode::decode_all(f).unwrap().1);
    }
    let got = captured.lock().clone();
    assert_eq!(got.len(), want.len());
    assert!(got == want, "세 곡이 이음매 없이 이어져야 한다");
    let events = p.state().events;
    assert!(advanced_to(&events, &item(&t[1]).uid) && advanced_to(&events, &item(&t[2]).uid));
}

#[test]
fn next_and_previous_buttons() {
    if common::samples().is_none() {
        return;
    }
    let t = tracks("Neon Harbor/2023 - Night Drive");
    let (p, _) = player_with(96_000, true, Duration::ZERO, Duration::ZERO);
    let (a, b, c) = (item(&t[0]), item(&t[1]), item(&t[2]));
    p.load(a.clone(), true, 0.0, false);
    p.set_queue(vec![b.clone(), c.clone()], vec![], false);
    wait_for(&p, |_| p.state().status == Status::Playing);
    // 다음: b로, 화면에는 넘어갔다고 알린다.
    p.next();
    wait_for(&p, |e| advanced_to(e, &b.uid));
    wait_for(&p, |_| p.state().uid.as_deref() == Some(b.uid.as_str()) && p.state().status == Status::Playing);
    // 바로 이전(3초 안): a로 돌아간다.
    p.prev();
    wait_for(&p, |_| p.state().uid.as_deref() == Some(a.uid.as_str()) && p.state().status == Status::Playing);
    // 다시 다음, 다음: b, c
    p.next();
    wait_for(&p, |_| p.state().uid.as_deref() == Some(b.uid.as_str()));
    p.next();
    wait_for(&p, |_| p.state().uid.as_deref() == Some(c.uid.as_str()));
    // 더 갈 곳이 없으면 그대로
    p.next();
    std::thread::sleep(Duration::from_millis(200));
    assert_eq!(p.state().uid.as_deref(), Some(c.uid.as_str()));
    // 이름표가 따라온다
    let mut m = item(&t[0]);
    m.uid = "with-meta".into();
    m.meta = Some(sori_core::player::Meta {
        title: "Harbor Lights".into(),
        artist: "Neon Harbor".into(),
        album: "Night Drive".into(),
        cover: None,
    });
    p.load(m, true, 0.0, false);
    wait_for(&p, |_| p.state().meta.as_ref().is_some_and(|m| m.title == "Harbor Lights"));
}

#[test]
fn repeat_one_loops_without_a_gap() {
    if common::samples().is_none() {
        return;
    }
    let t = tracks("하늘 사중주/아침의 차"); // 44.1kHz
    let (p, captured) = player(44_100);
    p.set_queue(vec![], vec![], true);
    p.load(item(&t[0]), true, 0.0, false);
    let (_, a) = decode::decode_all(&t[0]).unwrap();
    let t0 = Instant::now();
    while captured.lock().len() < a.len() * 2 + 4096 {
        assert!(t0.elapsed() < Duration::from_secs(60), "되풀이하지 않습니다");
        std::thread::sleep(Duration::from_millis(10));
    }
    p.stop();
    let got = captured.lock().clone();
    assert!(got[..a.len()] == a[..], "첫 바퀴");
    assert!(got[a.len()..a.len() * 2] == a[..], "둘째 바퀴가 첫 바퀴 바로 다음 샘플에 붙어야 한다");
    assert!(!ended(&p.state().events), "되풀이 중에는 끝나지 않는다");
}
