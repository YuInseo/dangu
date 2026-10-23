//! 자동 업데이트: GitHub 릴리스(`dambi-latest`)에 새 빌드가 올라오면 받아서 실행 파일을 갈아 끼우고
//! 다시 켭니다.
//!
//! - 이 빌드가 누구인지는 CI가 넣는 커밋 번호(`DAMBI_BUILD`)로 압니다. 직접 빌드한 것(번호 없음)은
//!   업데이트하지 않습니다 — 개발 중인 코드를 릴리스로 덮으면 안 되니까.
//! - 릴리스에는 `dambi-build.txt`(커밋 번호)와 실행 파일(`mc-server.exe` / `mc-server-linux`)이 있습니다.
//! - 켤 때: 새 빌드가 있으면 받아서 바꾸고 곧바로 다시 켬 (아직 아무도 없으니).
//! - 도는 중 (30분마다): 받아 두고(`<실행 파일>.new`) 알림. 접속자가 없으면 저장하고 다시 켜서
//!   적용, 있으면 다음에 켤 때 적용. `/업데이트` (OP 4, 콘솔)는 지금 저장하고 다시 켬.
//! - 윈도우는 도는 실행 파일을 지울 수는 없어도 이름은 바꿀 수 있어, `.old`로 비켜 두고 새 것을 그
//!   자리에 둡니다 (다음에 켤 때 `.old`를 지움).
//! - `MC_AUTO_UPDATE=false`면 끔. `MC_UPDATE_REPO`(기본 `YuInseo/dangu`), `MC_UPDATE_TAG`(기본
//!   `dambi-latest`).

use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

/// CI가 넣는 이 빌드의 커밋 번호.
pub const BUILD: Option<&str> = option_env!("DAMBI_BUILD");

/// 시작 스크립트(`서버 켜기.bat`, `start.sh`)가 "다시 켜 달라"로 알아듣는 종료 코드.
pub const RESTART_EXIT_CODE: i32 = 75;

const BUILD_ASSET: &str = "dambi-build.txt";

static RESTART: AtomicBool = AtomicBool::new(false);
static CHECK_NOW: AtomicBool = AtomicBool::new(false);

/// `/업데이트`: 다음 살피기 때 곧바로 확인하고, 있으면 접속자가 있어도 다시 켬.
pub fn request_check_now() {
    CHECK_NOW.store(true, Ordering::SeqCst);
}

pub fn take_check_now() -> bool {
    CHECK_NOW.swap(false, Ordering::SeqCst)
}

/// 확인해서 새 것이 있으면 받아 둠 (블로킹). 받아 둔 것이 (이미) 있으면 그 커밋 번호.
pub fn fetch_if_newer() -> Result<Option<String>, String> {
    if staged() {
        return Ok(Some("받아 둔 판".into()));
    }
    match check()? {
        None => Ok(None),
        Some(a) => {
            download(&a)?;
            Ok(Some(short(&a.build).to_owned()))
        }
    }
}

/// 이 플랫폼의 실행 파일 이름 (릴리스 자산).
pub fn asset_name() -> &'static str {
    if cfg!(windows) {
        "mc-server.exe"
    } else {
        "mc-server-linux"
    }
}

pub fn enabled() -> bool {
    BUILD.is_some()
        && std::env::var("MC_AUTO_UPDATE")
            .map_or(true, |v| !matches!(v.as_str(), "0" | "false" | "off"))
}

fn repo() -> String {
    std::env::var("MC_UPDATE_REPO").unwrap_or_else(|_| "YuInseo/dangu".into())
}

fn tag() -> String {
    std::env::var("MC_UPDATE_TAG").unwrap_or_else(|_| "dambi-latest".into())
}

/// 짧은 커밋 번호 (보여 주기용).
pub fn short(b: &str) -> &str {
    &b[..b.len().min(7)]
}

// ---- HTTPS (리다이렉트 따라가고, 본문은 바이트로) ----

fn split_url(url: &str) -> Option<(String, String)> {
    let rest = url.strip_prefix("https://")?;
    let (host, path) = rest.split_once('/').unwrap_or((rest, ""));
    Some((host.to_owned(), format!("/{path}")))
}

/// 상태, 머리(소문자 이름, 값), 본문.
type Response = (u16, Vec<(String, String)>, Vec<u8>);

fn parse(raw: &[u8]) -> Result<Response, String> {
    let split = raw
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .ok_or("HTTP 머리 끝이 없습니다")?;
    let head = String::from_utf8_lossy(&raw[..split]);
    let mut lines = head.split("\r\n");
    let status: u16 = lines
        .next()
        .and_then(|l| l.split(' ').nth(1))
        .and_then(|s| s.parse().ok())
        .ok_or("상태 줄이 이상합니다")?;
    let headers: Vec<(String, String)> = lines
        .filter_map(|l| l.split_once(':'))
        .map(|(k, v)| (k.trim().to_ascii_lowercase(), v.trim().to_owned()))
        .collect();
    let mut body = raw[split + 4..].to_vec();
    if headers
        .iter()
        .any(|(k, v)| k == "transfer-encoding" && v.to_ascii_lowercase().contains("chunked"))
    {
        let mut out = Vec::new();
        let mut rest = &body[..];
        loop {
            let end = rest
                .windows(2)
                .position(|w| w == b"\r\n")
                .ok_or("chunk 크기 줄")?;
            let size = usize::from_str_radix(
                String::from_utf8_lossy(&rest[..end])
                    .split(';')
                    .next()
                    .unwrap_or("")
                    .trim(),
                16,
            )
            .map_err(|_| "chunk 크기")?;
            rest = &rest[end + 2..];
            if size == 0 {
                break;
            }
            out.extend_from_slice(rest.get(..size).ok_or("chunk가 잘렸습니다")?);
            rest = rest.get(size + 2..).unwrap_or_default();
        }
        body = out;
    }
    Ok((status, headers, body))
}

/// GET 하나 (블로킹). 3xx면 `Location`을 다섯 번까지 따라갑니다.
pub fn get(url: &str, accept: &str) -> Result<Vec<u8>, String> {
    let mut url = url.to_owned();
    for _ in 0..5 {
        let (host, path) =
            split_url(&url).ok_or_else(|| format!("https 주소가 아닙니다: {url}"))?;
        let server_name =
            rustls::pki_types::ServerName::try_from(host.clone()).map_err(|e| e.to_string())?;
        let conn = rustls::ClientConnection::new(crate::auth::tls_config(), server_name)
            .map_err(|e| e.to_string())?;
        let tcp = TcpStream::connect((host.as_str(), 443)).map_err(|e| format!("{host}: {e}"))?;
        tcp.set_read_timeout(Some(Duration::from_secs(30))).ok();
        tcp.set_write_timeout(Some(Duration::from_secs(30))).ok();
        let mut tls = rustls::StreamOwned::new(conn, tcp);
        let request = format!(
            "GET {path} HTTP/1.1\r\nHost: {host}\r\nUser-Agent: dangu-mc-updater\r\nAccept: {accept}\r\nConnection: close\r\n\r\n"
        );
        tls.write_all(request.as_bytes())
            .map_err(|e| e.to_string())?;
        let mut raw = Vec::new();
        match tls.read_to_end(&mut raw) {
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof && !raw.is_empty() => {}
            Err(e) => return Err(e.to_string()),
        }
        let (status, headers, body) = parse(&raw)?;
        match status {
            200 => return Ok(body),
            301 | 302 | 303 | 307 | 308 => {
                url = headers
                    .iter()
                    .find(|(k, _)| k == "location")
                    .map(|(_, v)| v.clone())
                    .ok_or("Location 없는 리다이렉트")?;
            }
            other => return Err(format!("{host}: HTTP {other}")),
        }
    }
    Err("리다이렉트가 너무 많습니다".into())
}

// ---- 확인·받기·바꾸기 ----

/// 새 빌드: (커밋 번호, 실행 파일 주소).
#[derive(Debug, Clone, PartialEq)]
pub struct Available {
    pub build: String,
    pub url: String,
}

/// 릴리스 JSON에서 커밋 번호 파일과 이 플랫폼 실행 파일의 주소.
pub fn assets(release: &crate::json::Json) -> Option<(String, String)> {
    use crate::json::Json;
    let list = release.get("assets")?.as_array()?;
    let url = |name: &str| {
        list.iter()
            .find(|a| a.get("name").and_then(Json::as_str) == Some(name))
            .and_then(|a| a.get("browser_download_url")?.as_str().map(str::to_owned))
    };
    Some((url(BUILD_ASSET)?, url(asset_name())?))
}

/// 릴리스에 이 빌드와 다른 것이 있나 (블로킹).
pub fn check() -> Result<Option<Available>, String> {
    let Some(mine) = BUILD else {
        return Ok(None);
    };
    let api = format!(
        "https://api.github.com/repos/{}/releases/tags/{}",
        repo(),
        tag()
    );
    let body = get(&api, "application/vnd.github+json")?;
    let release = crate::json::parse(&String::from_utf8_lossy(&body))
        .map_err(|e| format!("릴리스 JSON: {e:?}"))?;
    let Some((build_url, exe_url)) = assets(&release) else {
        return Ok(None);
    };
    let build = String::from_utf8_lossy(&get(&build_url, "*/*")?)
        .trim()
        .to_owned();
    if build.is_empty() || build == mine {
        return Ok(None);
    }
    Ok(Some(Available {
        build,
        url: exe_url,
    }))
}

/// 실행 파일 자리. 처음 물을 때 한 번 정해 둡니다: 갈아 끼운 뒤에는 운영체제가 "지금 도는
/// 파일"을 `.old` 쪽으로 알려 주므로, 그때 물으면 옛 판을 다시 켜게 됩니다.
fn exe() -> Result<PathBuf, String> {
    static EXE: std::sync::OnceLock<Result<PathBuf, String>> = std::sync::OnceLock::new();
    EXE.get_or_init(|| std::env::current_exe().map_err(|e| e.to_string()))
        .clone()
}

fn with_suffix(p: &std::path::Path, suffix: &str) -> PathBuf {
    let mut s = p.as_os_str().to_owned();
    s.push(suffix);
    PathBuf::from(s)
}

/// 받아서 `<실행 파일>.new`로 둠 (블로킹).
pub fn download(a: &Available) -> Result<(), String> {
    let bytes = get(&a.url, "application/octet-stream")?;
    // 실행 파일이 맞는지 아주 조금 확인 (HTML 오류 페이지를 실행 파일로 두지 않게)
    let ok = if cfg!(windows) {
        bytes.starts_with(b"MZ")
    } else {
        bytes.starts_with(b"\x7fELF")
    };
    if !ok || bytes.len() < 1024 * 1024 {
        return Err("받은 파일이 실행 파일이 아닙니다".into());
    }
    let new = with_suffix(&exe()?, ".new");
    std::fs::write(&new, bytes).map_err(|e| format!("{}: {e}", new.display()))?;
    Ok(())
}

/// 받아 둔 새 실행 파일이 있나.
pub fn staged() -> bool {
    exe().is_ok_and(|e| with_suffix(&e, ".new").exists())
}

/// 받아 둔 것을 실행 파일 자리에 (지금 것은 `.old`로). 바꿨으면 true.
pub fn apply_staged() -> Result<bool, String> {
    let exe = exe()?;
    let new = with_suffix(&exe, ".new");
    if !new.exists() {
        return Ok(false);
    }
    let old = with_suffix(&exe, ".old");
    let _ = std::fs::remove_file(&old);
    std::fs::rename(&exe, &old).map_err(|e| format!("지금 실행 파일 비키기: {e}"))?;
    if let Err(e) = std::fs::rename(&new, &exe) {
        // 되돌림
        let _ = std::fs::rename(&old, &exe);
        return Err(format!("새 실행 파일 놓기: {e}"));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&exe, std::fs::Permissions::from_mode(0o755));
    }
    Ok(true)
}

/// 지난번에 비켜 둔 `.old` 지우기.
pub fn clean_old() {
    if let Ok(e) = exe() {
        let _ = std::fs::remove_file(with_suffix(&e, ".old"));
    }
}

/// 끄고 다시 켜 달라고 표시 (도는 중의 적용).
pub fn request_restart() {
    RESTART.store(true, Ordering::SeqCst);
}

pub fn restart_requested() -> bool {
    RESTART.load(Ordering::SeqCst)
}

/// 새 실행 파일로 다시 켜고 이 프로세스는 끝냄. 시작 스크립트 안이면 스크립트가 다시 켜도록
/// 약속한 코드로 끝내기만 합니다.
pub fn relaunch() -> ! {
    if std::env::var_os("DANGU_LAUNCHER").is_some() {
        std::process::exit(RESTART_EXIT_CODE);
    }
    let args: Vec<String> = std::env::args().skip(1).collect();
    match exe().and_then(|e| {
        std::process::Command::new(e)
            .args(&args)
            // 다시 켠 것: 브라우저를 또 열지 않음
            .env("DANGU_RELAUNCHED", "1")
            .spawn()
            .map_err(|e| e.to_string())
    }) {
        Ok(_) => std::process::exit(0),
        Err(e) => {
            eprintln!("다시 켜지 못했습니다: {e} — 직접 다시 켜 주세요.");
            std::process::exit(1)
        }
    }
}

/// 켤 때 (런타임을 만들기 전): 받아 둔 것이 있으면 바꾸고, 릴리스에 새 빌드가 있으면 받아서
/// 바꾼 뒤 다시 켬. 실패하면 알리고 그대로 켬.
pub fn on_startup() {
    clean_old();
    if !enabled() {
        return;
    }
    match apply_staged() {
        Ok(true) => {
            println!("[업데이트] 받아 둔 새 버전으로 바꿨습니다. 다시 켭니다…");
            relaunch();
        }
        Ok(false) => {}
        Err(e) => eprintln!("[업데이트] 바꾸지 못했습니다: {e}"),
    }
    print!(
        "[업데이트] 새 버전 확인 중 (지금 {})… ",
        short(BUILD.unwrap_or("?"))
    );
    let _ = std::io::stdout().flush();
    match check() {
        Ok(None) => println!("최신입니다."),
        Ok(Some(a)) => {
            println!("새 버전 {}을(를) 받습니다.", short(&a.build));
            match download(&a).and_then(|()| apply_staged()) {
                Ok(true) => {
                    println!("[업데이트] 바꿨습니다. 다시 켭니다…");
                    relaunch();
                }
                Ok(false) => {}
                Err(e) => eprintln!("[업데이트] 실패: {e} — 지금 버전으로 켭니다."),
            }
        }
        Err(e) => println!("확인하지 못했습니다 ({e}) — 지금 버전으로 켭니다."),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redirect_and_chunked_responses_parse() {
        let (s, h, b) = parse(
            b"HTTP/1.1 302 Found\r\nLocation: https://objects.example.com/x?y=1\r\nContent-Length: 0\r\n\r\n",
        )
        .unwrap();
        assert_eq!(s, 302);
        assert_eq!(
            h.iter().find(|(k, _)| k == "location").unwrap().1,
            "https://objects.example.com/x?y=1"
        );
        assert!(b.is_empty());
        let (_, _, b) =
            parse(b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n3\r\nabc\r\n2\r\nde\r\n0\r\n\r\n")
                .unwrap();
        assert_eq!(b, b"abcde");
        assert_eq!(
            split_url("https://api.github.com/repos/a/b"),
            Some(("api.github.com".into(), "/repos/a/b".into()))
        );
    }

    #[test]
    fn release_assets_are_found_by_name() {
        let j = crate::json::parse(&format!(
            r#"{{"assets":[{{"name":"dambi-build.txt","browser_download_url":"https://x/b"}},
                {{"name":"{}","browser_download_url":"https://x/e"}},
                {{"name":"other.zip","browser_download_url":"https://x/z"}}]}}"#,
            asset_name()
        ))
        .unwrap();
        assert_eq!(
            assets(&j),
            Some(("https://x/b".into(), "https://x/e".into()))
        );
        assert_eq!(
            assets(&crate::json::parse(r#"{"assets":[]}"#).unwrap()),
            None
        );
    }
}
