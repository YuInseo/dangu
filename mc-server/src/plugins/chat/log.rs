//! 채팅 기록: 모든 채널 채팅·귓속말·`/me`·`/say`·명령어를 `world/chat.log`(`MC_CHAT_LOG`)에 한 줄
//! JSON으로 붙여 씁니다. 여러 인스턴스(담비 자식)가 같은 파일에 쓰고, 웹 관리 화면 "채팅" 탭이
//! 1초마다 새 줄을 읽어 갑니다. 20MB를 넘으면 `chat.log.1`로 넘기고 새로 시작합니다.
//!
//! 한 줄: `{"t": 밀리초, "kind": "전체"|"귓속말"|…, "instance": …, "from": 이름, "to": 받는 이,
//! "guild": 길드, "text": 내용}`

use std::io::{Read, Seek, SeekFrom, Write};
use std::path::PathBuf;

use crate::json::Json;
use crate::plugins::store::data_path;

const ROTATE_AT: u64 = 20 * 1024 * 1024;
/// 처음 열 때 보여 줄 끝부분.
const TAIL_BYTES: u64 = 256 * 1024;

pub fn path() -> PathBuf {
    data_path("MC_CHAT_LOG", "chat.log")
}

/// 한 줄 남기기 (실패해도 게임은 그대로).
pub fn record(
    kind: &str,
    instance: &str,
    from: &str,
    to: Option<&str>,
    guild: Option<&str>,
    text: &str,
) {
    record_to(&path(), kind, instance, from, to, guild, text);
}

#[allow(clippy::too_many_arguments)]
fn record_to(
    p: &std::path::Path,
    kind: &str,
    instance: &str,
    from: &str,
    to: Option<&str>,
    guild: Option<&str>,
    text: &str,
) {
    let ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0.0, |d| d.as_millis() as f64);
    let mut f = vec![
        ("t".to_owned(), Json::Number(ms)),
        ("kind".into(), Json::String(kind.to_owned())),
        ("instance".into(), Json::String(instance.to_owned())),
        ("from".into(), Json::String(from.to_owned())),
    ];
    if let Some(t) = to {
        f.push(("to".into(), Json::String(t.to_owned())));
    }
    if let Some(g) = guild {
        f.push(("guild".into(), Json::String(g.to_owned())));
    }
    f.push(("text".into(), Json::String(text.to_owned())));
    let line = format!("{}\n", Json::Object(f).compact());
    if let Some(d) = p.parent() {
        let _ = std::fs::create_dir_all(d);
    }
    if std::fs::metadata(p).is_ok_and(|m| m.len() > ROTATE_AT) {
        let _ = std::fs::rename(p, p.with_extension("log.1"));
    }
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(p)
    {
        // 한 번에 써서 다른 프로세스의 줄과 섞이지 않게
        let _ = file.write_all(line.as_bytes());
    }
}

/// `after` 바이트 뒤의 새 줄들과 다음에 물을 자리. `after`가 없거나 파일보다 크면(새로 시작함)
/// 끝부분부터.
pub fn read_since(after: Option<u64>) -> (Vec<Json>, u64) {
    read_from(&path(), after)
}

fn read_from(p: &std::path::Path, after: Option<u64>) -> (Vec<Json>, u64) {
    let Ok(mut f) = std::fs::File::open(p) else {
        return (Vec::new(), 0);
    };
    let len = f.metadata().map_or(0, |m| m.len());
    let start = match after {
        Some(a) if a <= len => a,
        _ => len.saturating_sub(TAIL_BYTES),
    };
    if f.seek(SeekFrom::Start(start)).is_err() {
        return (Vec::new(), len);
    }
    let mut buf = Vec::new();
    let _ = f.take(len - start).read_to_end(&mut buf);
    // 끝이 줄바꿈으로 안 끝났으면 (쓰는 중) 그 줄은 다음에
    let complete = buf.iter().rposition(|&b| b == b'\n').map_or(0, |i| i + 1);
    let text = String::from_utf8_lossy(&buf[..complete]);
    let mut lines: Vec<&str> = text.lines().collect();
    // 끝부분부터 읽으면 첫 줄은 잘렸을 수 있음
    if after.is_none_or(|a| a > len) && start > 0 && !lines.is_empty() {
        lines.remove(0);
    }
    let out = lines
        .into_iter()
        .filter_map(|l| crate::json::parse(l).ok())
        .collect();
    (out, start + complete as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn appends_and_reads_new_lines_only() {
        let dir = std::env::temp_dir().join(format!("dangu-chatlog-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("chat.log");
        assert_eq!(read_from(&p, None).0.len(), 0);
        record_to(&p, "전체", "main", "Alex", None, None, "안녕");
        record_to(&p, "귓속말", "main", "Alex", Some("Steve"), None, "비밀");
        let (all, at) = read_from(&p, None);
        assert_eq!(all.len(), 2);
        assert_eq!(all[1].get("to").and_then(Json::as_str), Some("Steve"));
        assert_eq!(read_from(&p, Some(at)).0.len(), 0);
        record_to(&p, "길드", "town", "Steve", None, Some("달빛"), "모여");
        let (new, _) = read_from(&p, Some(at));
        assert_eq!(new.len(), 1);
        assert_eq!(new[0].get("text").and_then(Json::as_str), Some("모여"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
