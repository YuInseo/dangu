//! 시험용 가짜 archive.org: 이 컴퓨터 안의 작은 HTTP 서버.
//!
//! `/download/…`은 `/items/…`로 옮겨 보내고(302), 범위 요청(Range)을 받는다. `cut`을 주면
//! 본문을 그만큼만 보내고 연결을 끊는다(끊긴 연결을 이어 받는지 본다).
#![allow(dead_code)]

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

pub struct Fake {
    pub base: String,
    pub requests: Arc<AtomicUsize>,
}

impl Fake {
    pub fn requests(&self) -> usize {
        self.requests.load(Ordering::SeqCst)
    }
}

/// `files`: 경로 → 내용. `/download/…`은 `/items/…`로 옮겨 보낸다. `cut`이 있으면 본문을 그만큼만
/// 보내고 연결을 끊는다(끊긴 연결을 이어 받는지 본다).
pub fn serve(files: HashMap<String, Vec<u8>>, cut: Option<usize>) -> Fake {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let files = Arc::new(files);
    let requests = Arc::new(AtomicUsize::new(0));
    let count = requests.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let files = files.clone();
            let count = count.clone();
            std::thread::spawn(move || {
                let _ = handle(stream, &files, cut, &count);
            });
        }
    });
    Fake { base, requests }
}

fn handle(mut stream: TcpStream, files: &HashMap<String, Vec<u8>>, cut: Option<usize>, count: &AtomicUsize) -> std::io::Result<()> {
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut line = String::new();
    reader.read_line(&mut line)?;
    let path = line.split_whitespace().nth(1).unwrap_or("/").to_string();
    let mut range: Option<(u64, Option<u64>)> = None;
    loop {
        let mut h = String::new();
        if reader.read_line(&mut h)? == 0 || h == "\r\n" {
            break;
        }
        if let Some(v) = h.to_ascii_lowercase().strip_prefix("range: bytes=") {
            let (a, b) = v.trim().split_once('-').unwrap();
            range = Some((a.parse().unwrap(), b.parse().ok()));
        }
    }
    count.fetch_add(1, Ordering::SeqCst);
    let (route, _query) = path.split_once('?').unwrap_or((&path, ""));
    if let Some(rest) = route.strip_prefix("/download/") {
        let head = format!("HTTP/1.1 302 Found\r\nLocation: /items/{rest}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
        return stream.write_all(head.as_bytes());
    }
    let Some(body) = files.get(route) else {
        return stream.write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
    };
    let total = body.len() as u64;
    let (status, start, end) = match range {
        Some((a, b)) if a < total => ("206 Partial Content", a, b.unwrap_or(total - 1).min(total - 1)),
        Some(_) => return stream.write_all(b"HTTP/1.1 416 Range Not Satisfiable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"),
        None => ("200 OK", 0, total.saturating_sub(1)),
    };
    let slice = &body[start as usize..=end as usize];
    let mut head = format!("HTTP/1.1 {status}\r\nContent-Length: {}\r\nAccept-Ranges: bytes\r\nConnection: close\r\n", slice.len());
    if status.starts_with("206") {
        head.push_str(&format!("Content-Range: bytes {start}-{end}/{total}\r\n"));
    }
    head.push_str("\r\n");
    stream.write_all(head.as_bytes())?;
    let send = cut.map(|c| c.min(slice.len())).unwrap_or(slice.len());
    stream.write_all(&slice[..send])?;
    stream.flush()
}
