//! 곡과 표지를 보낸다.
//!
//! 기본은 디스크의 바이트를 그대로: 디코딩도, 재인코딩도, 리샘플링도 없다. 브라우저가 받는
//! 것이 파일 그 자체라는 뜻이고, 신호 경로의 "비트퍼펙트"가 가리키는 것이 이것이다.
//! 브라우저가 못 여는 형식은 `?transcode=1`로 받으면 소리가 직접 풀어 WAV로 흘려보낸다.

use std::io::SeekFrom;
use std::path::PathBuf;

use axum::Router;
use axum::body::{Body, Bytes};
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use futures_util::stream;
use serde::Deserialize;
use sori_core::covers::{CoverSize, cover_file};
use sori_core::decode;
use sori_core::formats::base_mime;
use tokio::io::{AsyncReadExt, AsyncSeekExt};
use tower_http::compression::predicate::{DefaultPredicate, NotForContentType, Predicate};

use crate::App;
use crate::wav::header as wav_header;
use sori_core::dsp::quantize::Quantizer;

pub fn compressible() -> impl Predicate {
    DefaultPredicate::new().and(NotForContentType::const_new("audio/"))
}

fn text(status: StatusCode, msg: &str) -> Response {
    (status, [(header::CONTENT_TYPE, "text/plain; charset=utf-8")], msg.to_string()).into_response()
}

#[derive(Deserialize, Default)]
struct StreamQuery {
    transcode: Option<String>,
    /// DSP 요청(base64url JSON). 있으면 64비트 체인을 거쳐 WAV로 보낸다.
    dsp: Option<String>,
    /// 클리핑 표시기 세션
    sid: Option<String>,
    /// 이 초부터(변환 스트림만)
    t: Option<f64>,
}

/// `bytes=a-b`, `bytes=a-`, `bytes=-n` 하나만. 여러 구간은 브라우저가 보내지 않는다.
pub fn parse_range(v: &str, size: u64) -> Option<Result<(u64, u64), ()>> {
    let r = v.trim().strip_prefix("bytes=")?;
    if r.contains(',') {
        return None;
    }
    let (a, b) = r.split_once('-')?;
    let (start, end) = match (a.trim(), b.trim()) {
        ("", "") => return None,
        ("", n) => {
            let n: u64 = n.parse().ok()?;
            (size.saturating_sub(n), size.saturating_sub(1))
        }
        (s, "") => (s.parse().ok()?, size.saturating_sub(1)),
        (s, e) => (s.parse().ok()?, e.parse::<u64>().ok()?.min(size.saturating_sub(1))),
    };
    if start >= size || start > end {
        return Some(Err(()));
    }
    Some(Ok((start, end)))
}

async fn file_body(path: PathBuf, start: u64, len: u64) -> std::io::Result<Body> {
    let mut f = tokio::fs::File::open(&path).await?;
    f.seek(SeekFrom::Start(start)).await?;
    let reader = tokio_util::io::ReaderStream::with_capacity(f.take(len), 256 * 1024);
    Ok(Body::from_stream(reader))
}

async fn stream_track(State(app): State<App>, Path(id): Path<String>, Query(q): Query<StreamQuery>, headers: HeaderMap) -> Response {
    let Some((abs, track)) = app.store.track_file(&id) else { return text(StatusCode::NOT_FOUND, "없는 곡입니다") };
    let at = q.t.unwrap_or(0.0).max(0.0);
    // 요청이 이상하면 무시하고 원본(또는 변환)으로 간다.
    if let Some(req) = q.dsp.as_deref().and_then(sori_core::dsp::request::parse_request) {
        let loudness = app.store.library().views.get(&id).and_then(|t| t.loudness.clone());
        let sid = q.sid.filter(|s| sori_core::dsp::chain::valid_sid(s));
        return pcm_stream(abs, at, Some(Dsp { req, loudness, sid, app: app.clone() })).await;
    }
    if q.transcode.is_some() {
        return pcm_stream(abs, at, None).await;
    }
    let meta = match tokio::fs::metadata(&abs).await {
        Ok(m) => m,
        Err(_) => return text(StatusCode::GONE, "파일이 사라졌습니다. 라이브러리를 다시 스캔하세요."),
    };
    let size = meta.len();
    let modified = meta.modified().ok();
    let etag = format!("\"{:x}-{:x}\"", track.mtime, size);
    let mut h = HeaderMap::new();
    h.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_str(base_mime(&track.format.mime)).unwrap_or(HeaderValue::from_static("application/octet-stream")),
    );
    h.insert(header::ACCEPT_RANGES, HeaderValue::from_static("bytes"));
    h.insert(header::CACHE_CONTROL, HeaderValue::from_static("private, max-age=86400"));
    h.insert(header::ETAG, HeaderValue::from_str(&etag).unwrap());
    if let Some(m) = modified {
        h.insert(header::LAST_MODIFIED, HeaderValue::from_str(&httpdate::fmt_http_date(m)).unwrap());
    }

    let if_range_ok = headers.get(header::IF_RANGE).is_none_or(|v| v.to_str().ok() == Some(etag.as_str()));
    if let Some(range) = headers.get(header::RANGE).and_then(|v| v.to_str().ok()).filter(|_| if_range_ok)
        && let Some(r) = parse_range(range, size)
    {
        let Ok((start, end)) = r else {
            return (StatusCode::RANGE_NOT_SATISFIABLE, [(header::CONTENT_RANGE, format!("bytes */{size}"))]).into_response();
        };
        let len = end - start + 1;
        h.insert(header::CONTENT_RANGE, HeaderValue::from_str(&format!("bytes {start}-{end}/{size}")).unwrap());
        h.insert(header::CONTENT_LENGTH, HeaderValue::from(len));
        return match file_body(abs, start, len).await {
            Ok(body) => (StatusCode::PARTIAL_CONTENT, h, body).into_response(),
            Err(_) => text(StatusCode::GONE, "파일을 열 수 없습니다"),
        };
    }
    if headers.get(header::IF_NONE_MATCH).and_then(|v| v.to_str().ok()) == Some(etag.as_str()) {
        return (StatusCode::NOT_MODIFIED, h).into_response();
    }
    h.insert(header::CONTENT_LENGTH, HeaderValue::from(size));
    match file_body(abs, 0, size).await {
        Ok(body) => (StatusCode::OK, h, body).into_response(),
        Err(_) => text(StatusCode::GONE, "파일을 열 수 없습니다"),
    }
}

struct Dsp {
    req: sori_core::dsp::request::DspRequest,
    loudness: Option<sori_core::model::Loudness>,
    sid: Option<String>,
    app: App,
}

/// 풀어서 WAV로. DSP가 없으면 원본이 정수 PCM일 때 샘플이 그대로 간다.
async fn pcm_stream(abs: PathBuf, at: f64, dsp: Option<Dsp>) -> Response {
    let opened = tokio::task::spawn_blocking(move || -> anyhow::Result<_> {
        let mut src = decode::open(&abs)?;
        let spec = src.spec().clone();
        let start = (at * spec.rate as f64).round() as u64;
        if start > 0 {
            src.seek(start)?;
        }
        Ok((src, spec, start))
    })
    .await;
    let (mut src, spec, start) = match opened {
        Ok(Ok(v)) => v,
        Ok(Err(e)) => return text(StatusCode::UNSUPPORTED_MEDIA_TYPE, &format!("이 파일을 풀 수 없습니다: {e}")),
        Err(_) => return text(StatusCode::INTERNAL_SERVER_ERROR, "변환 작업이 멈췄습니다"),
    };
    let remaining_in = spec.frames.map(|f| f.saturating_sub(start));
    let (bits, channels, rate, dither, mut proc, sid, clips) = match &dsp {
        Some(d) => {
            let p = sori_core::dsp::stream::Processor::new(&d.req, &spec, d.loudness.as_ref(), at, Some(&d.app.irs));
            for w in p.warnings() {
                tracing::warn!("{w}");
            }
            if let Some(sid) = &d.sid {
                d.app.clips.reset_after(sid, at);
            }
            let dither = d.req.out.d == sori_core::dsp::request::Dither::Tpdf;
            (d.req.out.b as u16, 2u16, p.out_rate, dither, Some(p), d.sid.clone(), Some(d.app.clips.clone()))
        }
        // 16비트 이하 원본은 16비트로, 나머지는 24비트로. 디더는 샘플이 정수 격자 밖에 있을 때만
        // 들어간다(DSD를 푼 것, 32비트·부동소수점 원본). 16/24비트 원본은 그대로 간다.
        None => {
            let bits = match spec.bits {
                Some(b) if b <= 16 && spec.dsd_rate.is_none() => 16u16,
                _ => 24u16,
            };
            (bits, spec.channels as u16, spec.rate, true, None, None, None)
        }
    };
    let block = channels as u64 * bits as u64 / 8;
    let remaining = remaining_in.map(|f| (f as u128 * rate as u128 / spec.rate as u128) as u64);
    let data_len = remaining.map(|f| f * block).unwrap_or(u32::MAX as u64 / block * block);

    let (tx, rx) = tokio::sync::mpsc::channel::<Bytes>(8);
    let known = spec.frames.is_some();
    tokio::task::spawn_blocking(move || {
        if tx.blocking_send(Bytes::from(wav_header(rate, channels, bits, data_len))).is_err() {
            return;
        }
        let mut q = Quantizer::new(bits as u32, dither);
        let mut pcm = Vec::with_capacity(1 << 16);
        let mut processed = Vec::with_capacity(1 << 17);
        let mut out = Vec::with_capacity(1 << 18);
        let mut sent = 0u64;
        let mut done = false;
        while !done {
            pcm.clear();
            processed.clear();
            let n = src.read(&mut pcm).unwrap_or(0);
            let samples: &[f64] = match &mut proc {
                Some(p) => {
                    if n == 0 {
                        p.finish(&mut processed);
                        done = true;
                    } else {
                        p.push(&pcm, &mut processed);
                    }
                    if let (Some((times, peak)), Some(sid), Some(clips)) = (p.take_clips(), &sid, &clips) {
                        clips.record(sid, &times, peak);
                    }
                    &processed
                }
                None => {
                    if n == 0 {
                        done = true;
                    }
                    &pcm
                }
            };
            out.clear();
            q.push_le(samples, &mut out);
            // 머리에 적은 길이를 넘기지 않는다(길이를 몰랐던 파일).
            let take = (out.len() as u64).min(data_len - sent) as usize;
            sent += take as u64;
            if take > 0 && tx.blocking_send(Bytes::copy_from_slice(&out[..take])).is_err() {
                return;
            }
            if sent >= data_len {
                return;
            }
        }
        // 실제가 머리보다 짧으면 무음으로 채워 길이를 맞춘다(브라우저가 끝을 기다리지 않게).
        let mut left = data_len - sent;
        let zeros = vec![0u8; 1 << 16];
        while left > 0 && known {
            let n = left.min(zeros.len() as u64) as usize;
            if tx.blocking_send(Bytes::copy_from_slice(&zeros[..n])).is_err() {
                return;
            }
            left -= n as u64;
        }
    });
    let body = Body::from_stream(stream::unfold(rx, |mut rx| async move { rx.recv().await.map(|b| (Ok::<_, std::io::Error>(b), rx)) }));
    let mut h = HeaderMap::new();
    h.insert(header::CONTENT_TYPE, HeaderValue::from_static("audio/wav"));
    h.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    h.insert("x-sori-transcoded", HeaderValue::from_static("wav"));
    if dsp.is_some() {
        h.insert("x-sori-dsp", HeaderValue::from_static("1"));
    }
    if known {
        h.insert(header::CONTENT_LENGTH, HeaderValue::from(data_len + 44 + if channels > 2 { 24 } else { 0 }));
    }
    (StatusCode::OK, h, body).into_response()
}

#[derive(Deserialize)]
struct CoverQuery {
    s: Option<String>,
    v: Option<String>,
}

async fn cover(State(app): State<App>, Path(id): Path<String>, Query(q): Query<CoverQuery>) -> Response {
    let lib = app.store.library();
    let Some(c) = lib.albums.get(&id).and_then(|a| a.cover.clone()) else { return text(StatusCode::NOT_FOUND, "표지가 없습니다") };
    let size = CoverSize::parse(q.s.as_deref().unwrap_or("sm"));
    let path = cover_file(&app.store.covers_dir(), &id, &c.v, size);
    let Ok(bytes) = tokio::fs::read(&path).await else { return text(StatusCode::NOT_FOUND, "표지 파일이 없습니다") };
    // 주소에 표지 버전이 들어 있으면 영원히 캐시해도 된다.
    let cache = if q.v.as_deref() == Some(c.v.as_str()) { "public, max-age=31536000, immutable" } else { "public, max-age=300" };
    ([(header::CONTENT_TYPE, "image/jpeg"), (header::CACHE_CONTROL, cache)], bytes).into_response()
}

pub fn routes() -> Router<App> {
    Router::new().route("/api/stream/{id}", get(stream_track)).route("/api/cover/{id}", get(cover))
}

#[cfg(test)]
mod tests {
    use super::parse_range;

    #[test]
    fn ranges() {
        assert_eq!(parse_range("bytes=0-99", 1000), Some(Ok((0, 99))));
        assert_eq!(parse_range("bytes=900-", 1000), Some(Ok((900, 999))));
        assert_eq!(parse_range("bytes=-100", 1000), Some(Ok((900, 999))));
        assert_eq!(parse_range("bytes=0-5000", 1000), Some(Ok((0, 999))));
        assert_eq!(parse_range("bytes=1000-", 1000), Some(Err(())));
        assert_eq!(parse_range("bytes=0-1,5-6", 1000), None);
        assert_eq!(parse_range("items=0-1", 1000), None);
    }
}
