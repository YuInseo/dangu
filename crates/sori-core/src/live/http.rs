//! 주소에서 받는 원본. 뒤에서 미리 받아 두고, 디코더는 받은 만큼 읽는다.
//!
//! 재생 스레드가 네트워크를 기다리며 멈추지 않도록 실제로 받는 일은 따로 도는 스레드가
//! 한다. 디코더가 멀리 건너뛰면(시크) 그 위치부터 새로 받는다(Range 요청). 받는 쪽은 읽는
//! 위치보다 너무 앞서 가지 않고(메모리), 읽은 지 오래된 앞부분은 버린다. 연결이 끊기면
//! 받은 데까지 이어서 다시 받는다.

use std::collections::VecDeque;
use std::io::{self, Read, Seek, SeekFrom};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{anyhow, bail};
use parking_lot::{Condvar, Mutex};
use symphonia::core::io::MediaSource;
use ureq::http::{HeaderMap, Response, header};

const CHUNK: usize = 64 * 1024;
/// 읽는 위치보다 이만큼까지 앞서 받는다(24비트 96kHz FLAC이면 40초쯤).
const MAX_AHEAD: u64 = 16 << 20;
/// 읽은 위치 뒤로 이만큼은 남겨 둔다(디코더가 조금 되돌아가 읽는다).
const KEEP_BEHIND: u64 = 1 << 20;
/// 받은 끝에서 이만큼 안쪽으로 건너뛰면 새로 요청하지 않고 기다린다.
const JUMP: u64 = 256 << 10;
/// 이만큼 앞서 받아 두었으면 읽어도 막히지 않는다고 본다(디코더는 한 번에 32KB까지 읽는다).
pub const READY_AHEAD: u64 = 128 << 10;
/// 읽는 쪽이 이만큼 기다렸는데 진척이 없으면 다시 연결한다.
const STALL: Duration = Duration::from_secs(12);
/// 이만큼 기다리면 포기한다.
const GIVE_UP: Duration = Duration::from_secs(45);
const MAX_REDIRECTS: usize = 6;
const MAX_FAILURES: u32 = 4;

/// 곡을 받는 데 쓰는 연결 설정. 본문은 길게 받으므로 전체 시간 제한은 넉넉히(끊기면 이어 받는다).
pub fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .user_agent(super::user_agent())
        .http_status_as_error(false)
        .max_redirects(0)
        .timeout_connect(Some(Duration::from_secs(10)))
        .timeout_recv_response(Some(Duration::from_secs(15)))
        .timeout_recv_body(Some(Duration::from_secs(30 * 60)))
        .build()
        .new_agent()
}

/// `Location`을 지금 주소에 대해 풀어 절대 주소로.
fn resolve(base: &str, loc: &str) -> anyhow::Result<String> {
    if loc.starts_with("http://") || loc.starts_with("https://") {
        return Ok(loc.to_string());
    }
    let (scheme, rest) = base.split_once("://").ok_or_else(|| anyhow!("주소가 이상합니다: {base}"))?;
    if let Some(l) = loc.strip_prefix("//") {
        return Ok(format!("{scheme}://{l}"));
    }
    let host_end = rest.find('/').unwrap_or(rest.len());
    let origin = format!("{scheme}://{}", &rest[..host_end]);
    if loc.starts_with('/') {
        return Ok(format!("{origin}{loc}"));
    }
    let path = &rest[host_end..];
    let path = path.split(['?', '#']).next().unwrap_or("");
    let dir = path.rfind('/').map(|i| &path[..=i]).unwrap_or("/");
    Ok(format!("{origin}{dir}{loc}"))
}

/// `from`부터 받는 요청. 옮겨 가라는 응답은 직접 따라가며 마지막 주소를 돌려준다
/// (같은 파일을 다시 받을 때 바로 그 주소로 간다).
fn get_from(agent: &ureq::Agent, url: &str, from: u64) -> anyhow::Result<(Response<ureq::Body>, String)> {
    let mut url = url.to_string();
    for _ in 0..=MAX_REDIRECTS {
        let res = agent
            .get(&url)
            .header(header::RANGE, format!("bytes={from}-"))
            .header(header::ACCEPT_ENCODING, "identity")
            .call()
            .map_err(|e| anyhow!("받을 수 없습니다: {e}"))?;
        let status = res.status();
        if status.is_redirection() {
            let loc =
                res.headers().get(header::LOCATION).and_then(|v| v.to_str().ok()).ok_or_else(|| anyhow!("옮겨 갈 주소가 없습니다"))?;
            url = resolve(&url, loc)?;
            continue;
        }
        if !status.is_success() {
            bail!("서버가 {}을(를) 돌려주었습니다", status.as_u16());
        }
        return Ok((res, url));
    }
    bail!("주소가 너무 여러 번 옮겨졌습니다")
}

fn header_u64(h: &HeaderMap, name: header::HeaderName) -> Option<u64> {
    h.get(name)?.to_str().ok()?.trim().parse().ok()
}

/// `Content-Range: bytes a-b/total` → (a, total)
fn content_range(h: &HeaderMap) -> Option<(u64, Option<u64>)> {
    let v = h.get(header::CONTENT_RANGE)?.to_str().ok()?.trim();
    let r = v.strip_prefix("bytes ")?;
    let (span, total) = r.split_once('/')?;
    let start = span.split_once('-')?.0.trim().parse().ok()?;
    Some((start, total.trim().parse().ok()))
}

/// 앞부분만(`limit` 바이트까지) 받는다. 범위 요청을 무시하는 서버도 있으니 거기서 끊는다.
pub fn fetch_head(agent: &ureq::Agent, url: &str, limit: usize) -> anyhow::Result<Vec<u8>> {
    let mut url = url.to_string();
    for _ in 0..=MAX_REDIRECTS {
        let res = agent
            .get(&url)
            .header(header::RANGE, format!("bytes=0-{}", limit.saturating_sub(1)))
            .header(header::ACCEPT_ENCODING, "identity")
            .call()
            .map_err(|e| anyhow!("받을 수 없습니다: {e}"))?;
        if res.status().is_redirection() {
            let loc =
                res.headers().get(header::LOCATION).and_then(|v| v.to_str().ok()).ok_or_else(|| anyhow!("옮겨 갈 주소가 없습니다"))?;
            url = resolve(&url, loc)?;
            continue;
        }
        if !res.status().is_success() {
            bail!("서버가 {}을(를) 돌려주었습니다", res.status().as_u16());
        }
        let mut out = Vec::with_capacity(limit);
        res.into_body().into_reader().take(limit as u64).read_to_end(&mut out)?;
        return Ok(out);
    }
    bail!("주소가 너무 여러 번 옮겨졌습니다")
}

struct State {
    /// 받아 둔 구간: [start, start + buf.len())
    start: u64,
    buf: VecDeque<u8>,
    /// 디코더가 마지막으로 읽은 위치
    reader: u64,
    /// 받는 스레드의 세대. 새로 받기 시작하면 오르고, 옛 스레드는 알아서 그만둔다.
    generation: u64,
    error: Option<String>,
    closed: bool,
    /// 마지막으로 무언가 받은 때
    progress: Instant,
}

impl State {
    fn end(&self) -> u64 {
        self.start + self.buf.len() as u64
    }
}

struct Inner {
    agent: ureq::Agent,
    /// 옮겨 간 뒤의 실제 주소
    url: String,
    len: u64,
    state: Mutex<State>,
    cond: Condvar,
}

impl Inner {
    /// `from`부터 이어지는 본문. 범위를 무시하고 처음부터 보내면 앞을 버린다.
    fn open_at(&self, from: u64) -> anyhow::Result<Box<dyn Read + Send>> {
        let (res, _) = get_from(&self.agent, &self.url, from)?;
        let partial = res.status().as_u16() == 206;
        let start = if partial { content_range(res.headers()).map(|(s, _)| s).unwrap_or(from) } else { 0 };
        let mut body: Box<dyn Read + Send> = Box::new(res.into_body().into_reader());
        if start > from {
            bail!("서버가 다른 위치부터 보냈습니다");
        }
        if start < from {
            let skip = from - start;
            let n = io::copy(&mut (&mut body).take(skip), &mut io::sink())?;
            if n != skip {
                bail!("파일이 생각보다 짧습니다");
            }
        }
        Ok(body)
    }
}

/// 받는 스레드 하나. `from`은 지금 받아 둔 구간의 끝이어야 한다.
fn spawn(inner: Arc<Inner>, generation: u64, from: u64, body: Option<Box<dyn Read + Send>>) {
    let r = std::thread::Builder::new().name("sori-http".into()).spawn(move || download(&inner, generation, from, body));
    if let Err(e) = r {
        tracing::warn!("받는 스레드를 만들 수 없습니다: {e}");
    }
}

fn download(inner: &Inner, generation: u64, mut offset: u64, mut body: Option<Box<dyn Read + Send>>) {
    let mut chunk = vec![0u8; CHUNK];
    let mut failures = 0u32;
    let fail = |msg: String| {
        let mut st = inner.state.lock();
        if st.generation == generation {
            st.error = Some(msg);
            inner.cond.notify_all();
        }
    };
    loop {
        // 그만둘 때인지, 너무 앞서 있는지
        {
            let mut st = inner.state.lock();
            loop {
                if st.closed || st.generation != generation || offset >= inner.len {
                    return;
                }
                if offset < st.reader.saturating_add(MAX_AHEAD) {
                    break;
                }
                inner.cond.wait_for(&mut st, Duration::from_millis(500));
            }
        }
        if body.is_none() {
            match inner.open_at(offset) {
                Ok(r) => body = Some(r),
                Err(e) => {
                    failures += 1;
                    if failures >= MAX_FAILURES {
                        fail(e.to_string());
                        return;
                    }
                    std::thread::sleep(Duration::from_millis(200 << failures));
                    continue;
                }
            }
        }
        let Some(reader) = body.as_mut() else { continue };
        let want = CHUNK.min((inner.len - offset) as usize);
        match reader.read(&mut chunk[..want]) {
            Ok(n) if n > 0 => {
                failures = 0;
                let mut st = inner.state.lock();
                if st.closed || st.generation != generation || st.end() != offset {
                    return;
                }
                st.buf.extend(&chunk[..n]);
                offset += n as u64;
                // 오래전에 읽은 앞부분은 버린다.
                let keep_from = st.reader.saturating_sub(KEEP_BEHIND);
                if keep_from > st.start {
                    let drop = ((keep_from - st.start) as usize).min(st.buf.len());
                    st.buf.drain(..drop);
                    st.start += drop as u64;
                }
                st.progress = Instant::now();
                inner.cond.notify_all();
            }
            other => {
                // 끊겼다(0바이트는 서버가 일찍 닫은 것). 받은 데부터 다시 연결한다.
                body = None;
                failures += 1;
                if failures >= MAX_FAILURES {
                    fail(match other {
                        Err(e) => format!("받다가 끊겼습니다: {e}"),
                        _ => "서버가 연결을 일찍 닫았습니다".into(),
                    });
                    return;
                }
                std::thread::sleep(Duration::from_millis(100 << failures));
            }
        }
    }
}

/// 디코더가 읽어도 막히지 않을지 묻는 손잡이. 재생 스레드가 읽기 전에 본다.
#[derive(Clone)]
pub struct Gate(Arc<Inner>);

impl Gate {
    pub fn ready(&self) -> bool {
        let st = self.0.state.lock();
        let end = st.end();
        st.error.is_some() || end >= self.0.len || (st.reader >= st.start && end.saturating_sub(st.reader) >= READY_AHEAD)
    }

    /// 읽는 위치 앞에 받아 둔 바이트 수.
    pub fn buffered(&self) -> u64 {
        let st = self.0.state.lock();
        if st.reader >= st.start { st.end().saturating_sub(st.reader) } else { 0 }
    }
}

pub struct HttpSource {
    inner: Arc<Inner>,
    pos: u64,
}

impl HttpSource {
    /// 첫 요청으로 길이와 실제 주소를 알아내고, 그 응답을 받는 스레드에 넘긴다.
    pub fn open(agent: &ureq::Agent, url: &str) -> anyhow::Result<HttpSource> {
        let (res, final_url) = get_from(agent, url, 0)?;
        let h = res.headers();
        let len = if res.status().as_u16() == 206 { content_range(h).and_then(|(_, t)| t) } else { header_u64(h, header::CONTENT_LENGTH) }
            .ok_or_else(|| anyhow!("길이를 알 수 없는 파일입니다"))?;
        let inner = Arc::new(Inner {
            agent: agent.clone(),
            url: final_url,
            len,
            state: Mutex::new(State {
                start: 0,
                buf: VecDeque::new(),
                reader: 0,
                generation: 0,
                error: None,
                closed: false,
                progress: Instant::now(),
            }),
            cond: Condvar::new(),
        });
        spawn(inner.clone(), 0, 0, Some(Box::new(res.into_body().into_reader())));
        Ok(HttpSource { inner, pos: 0 })
    }

    pub fn gate(&self) -> Gate {
        Gate(self.inner.clone())
    }

    pub fn len(&self) -> u64 {
        self.inner.len
    }

    pub fn is_empty(&self) -> bool {
        self.inner.len == 0
    }

    /// 지금 받아 둔 구간을 버리고 `at`부터 새로 받는다.
    fn restart(&self, st: &mut State, at: u64) {
        st.generation += 1;
        st.start = at;
        st.buf.clear();
        st.error = None;
        st.progress = Instant::now();
        spawn(self.inner.clone(), st.generation, at, None);
    }
}

impl Read for HttpSource {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        let len = self.inner.len;
        if out.is_empty() || self.pos >= len {
            return Ok(0);
        }
        let inner = self.inner.clone();
        let mut st = inner.state.lock();
        st.reader = self.pos;
        let began = Instant::now();
        let mut reconnects = 0;
        loop {
            let end = st.end();
            if self.pos >= st.start && self.pos < end {
                let off = (self.pos - st.start) as usize;
                let n = out.len().min(st.buf.len() - off);
                let (a, b) = st.buf.as_slices();
                if off < a.len() {
                    let k = n.min(a.len() - off);
                    out[..k].copy_from_slice(&a[off..off + k]);
                    out[k..n].copy_from_slice(&b[..n - k]);
                } else {
                    let o = off - a.len();
                    out[..n].copy_from_slice(&b[o..o + n]);
                }
                self.pos += n as u64;
                st.reader = self.pos;
                inner.cond.notify_all();
                return Ok(n);
            }
            if let Some(e) = &st.error {
                return Err(io::Error::other(e.clone()));
            }
            if self.pos < st.start || self.pos > end + JUMP {
                // 멀리 건너뛰었다: 거기서부터 새로 받는다.
                self.restart(&mut st, self.pos);
            } else if st.progress.max(began).elapsed() > STALL * (reconnects + 1) {
                // 받는 쪽이 멈췄다: 받은 끝에서 다시 연결한다.
                reconnects += 1;
                let at = st.end();
                st.generation += 1;
                st.progress = Instant::now();
                spawn(inner.clone(), st.generation, at, None);
            }
            if began.elapsed() > GIVE_UP {
                return Err(io::Error::new(io::ErrorKind::TimedOut, "네트워크가 응답하지 않습니다"));
            }
            inner.cond.wait_for(&mut st, Duration::from_millis(200));
        }
    }
}

impl Seek for HttpSource {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        let len = self.inner.len as i128;
        let to = match pos {
            SeekFrom::Start(n) => n as i128,
            SeekFrom::End(d) => len + d as i128,
            SeekFrom::Current(d) => self.pos as i128 + d as i128,
        };
        if to < 0 {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "파일 앞으로 갈 수 없습니다"));
        }
        self.pos = to as u64;
        Ok(self.pos)
    }
}

impl MediaSource for HttpSource {
    fn is_seekable(&self) -> bool {
        true
    }

    fn byte_len(&self) -> Option<u64> {
        Some(self.inner.len)
    }
}

impl Drop for HttpSource {
    fn drop(&mut self) {
        let mut st = self.inner.state.lock();
        st.closed = true;
        self.inner.cond.notify_all();
    }
}

#[cfg(test)]
mod tests {
    use super::resolve;

    #[test]
    fn locations() {
        let base = "https://archive.org/download/x/a.flac?y=1";
        assert_eq!(
            resolve(base, "https://ia800.us.archive.org/1/items/x/a.flac").unwrap(),
            "https://ia800.us.archive.org/1/items/x/a.flac"
        );
        assert_eq!(resolve(base, "//cdn.example/a").unwrap(), "https://cdn.example/a");
        assert_eq!(resolve(base, "/items/x/a.flac").unwrap(), "https://archive.org/items/x/a.flac");
        assert_eq!(resolve(base, "b.flac").unwrap(), "https://archive.org/download/x/b.flac");
        assert_eq!(resolve("http://127.0.0.1:9", "/z").unwrap(), "http://127.0.0.1:9/z");
    }
}
