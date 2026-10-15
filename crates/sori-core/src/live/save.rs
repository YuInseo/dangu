//! 공연을 라이브러리에 저장한다(내려받기).
//!
//! 아티스트가 내려받기를 허락한 공연만 받는다([`LiveShowDetail::saveable`]). 밴드가 듣기만
//! 허락한 공연(stream_only, 원본이 비공개)과 라이브 음악 아카이브 밖의 자료는 받지 않는다.
//!
//! 받는 곳은 `음악 폴더/Live Archive/<밴드>/<날짜 공연장>/<번호 곡>.flac`이고, 음악 폴더 바로
//! 아래에 Music 폴더가 있으면 그 아래다. 휴대폰에서 저장공간 전체를 음악 폴더로 골랐을 때를
//! 위해서다(안드로이드 11부터 앱은 Music 같은 정해진 폴더에만 음악 파일을 만들 수 있다).
//!
//! 곡은 숨은 이름(`.sori-01.flac`)으로 받고, archive.org가 알려 준 크기와 SHA-1을 맞춰 본 뒤
//! 태그(제목, 밴드, 공연, 트랙 번호, 날짜)를 적고 제 이름으로 바꾼다. 받다 만 파일에도 확장자를
//! 그대로 두는 것은 안드로이드가 음악 폴더에 음악이 아닌 파일을 만들지 못하게 해서다. 연결이
//! 끊기면 받은 데부터 이어 받는다.

use std::fmt;
use std::fs::{self, File};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use anyhow::{Context, bail};
use sha1::{Digest, Sha1};

use super::{Archive, LiveShowDetail, http};

pub const FOLDER: &str = "Live Archive";

/// 진척이 없는 시도를 이만큼 거듭하면 그만둔다(받는 게 있으면 몇 번이고 이어 받는다).
const MAX_STALLS: u32 = 5;

/// 저장할 곳의 맨 위.
pub fn root(music_dir: &Path) -> PathBuf {
    let music = music_dir.join("Music");
    if music.is_dir() { music.join(FOLDER) } else { music_dir.join(FOLDER) }
}

/// 무엇을 어디에 받을지.
#[derive(Clone, Debug, PartialEq)]
pub struct Plan {
    pub id: String,
    pub band: String,
    /// 앨범 이름이자 폴더 이름("1977-05-08 Barton Hall, Cornell University")
    pub album: String,
    pub date: Option<String>,
    pub dir: PathBuf,
    pub files: Vec<PlanFile>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PlanFile {
    /// 공연 안의 파일 이름(archive.org)
    pub file: String,
    pub title: String,
    /// 1부터. 여러 장으로 나뉜 공연도 이어서 센다.
    pub track: u32,
    /// 저장할 이름("01 Mars For The Rich.flac")
    pub name: String,
    pub size: Option<u64>,
    pub sha1: Option<String>,
}

impl Plan {
    /// 모든 파일의 크기를 알 때만
    pub fn total_bytes(&self) -> Option<u64> {
        self.files.iter().map(|f| f.size).sum()
    }
}

/// 받는 중의 진척
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Progress {
    pub files_done: usize,
    pub files_total: usize,
    pub bytes_done: u64,
    pub bytes_total: Option<u64>,
}

/// 취소했을 때의 오류
#[derive(Debug)]
pub struct Cancelled;

impl fmt::Display for Cancelled {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("취소했습니다")
    }
}

impl std::error::Error for Cancelled {}

/// 이 기기의 저장공간에 쓰지 못했다(다시 받아도 소용없다).
#[derive(Debug)]
struct DiskError(std::io::Error);

impl fmt::Display for DiskError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "저장공간에 쓰지 못했습니다: {}", self.0)
    }
}

impl std::error::Error for DiskError {}

/// 무엇을 어디에 받을지 정한다. `owner`는 그 폴더에 저장한 공연(기억해 둔 것). 같은 날 같은
/// 곳의 다른 녹음이 이미 있으면 이름 뒤에 (2), (3)을 붙인다. 앨범 이름도 같이 달라서
/// 라이브러리에서 두 녹음이 한 앨범으로 섞이지 않는다.
pub fn plan(show: &LiveShowDetail, root: &Path, owner: impl Fn(&Path) -> Option<String>) -> anyhow::Result<Plan> {
    if !show.saveable {
        bail!("{}", show.save_block.as_deref().unwrap_or(super::STREAM_ONLY));
    }
    let id = show.show.id.as_str();
    let band = clean_name(&show.show.band, "Unknown Artist", 60);
    let base = album_name(show);
    let mut n = 1;
    let (album, dir) = loop {
        let album = if n == 1 { base.clone() } else { format!("{base} ({n})") };
        let dir = root.join(&band).join(clean_name(&album, id, 90));
        let free = match owner(&dir) {
            Some(o) => o == id,
            None => !dir.exists(),
        };
        if free {
            break (album, dir);
        }
        n += 1;
        if n > 99 {
            bail!("저장할 폴더 이름을 정하지 못했습니다");
        }
    };
    let width = show.tracks.len().to_string().len().max(2);
    let files = show
        .tracks
        .iter()
        .enumerate()
        .map(|(i, t)| {
            let track = i as u32 + 1;
            let ext = extension(&t.file);
            let title = clean_name(&t.title, &format!("Track {track}"), 90);
            PlanFile {
                file: t.file.clone(),
                title: t.title.clone(),
                track,
                name: format!("{track:0width$} {title}.{ext}"),
                size: t.size,
                sha1: t.sha1.clone(),
            }
        })
        .collect();
    Ok(Plan { id: id.to_string(), band: show.show.band.trim().to_string(), album, date: show.show.date.clone(), dir, files })
}

/// "날짜 공연장". 공연장을 모르면 도시, 그것도 모르면 날짜만. 날짜를 모르면 공연 제목.
fn album_name(show: &LiveShowDetail) -> String {
    let s = &show.show;
    let place = s.venue.as_deref().or(s.city.as_deref()).map(str::trim).filter(|v| !v.is_empty());
    match (s.date.as_deref().filter(|d| !d.is_empty()), place) {
        (Some(d), Some(p)) => format!("{d} {p}"),
        (Some(d), None) => d.to_string(),
        (None, _) => s.title.trim().to_string(),
    }
}

fn extension(file: &str) -> String {
    let leaf = file.rsplit('/').next().unwrap_or(file);
    match leaf.rsplit_once('.') {
        Some((_, e)) if !e.is_empty() && e.len() <= 5 && e.bytes().all(|b| b.is_ascii_alphanumeric()) => e.to_ascii_lowercase(),
        _ => "flac".into(),
    }
}

/// 파일·폴더 이름으로 쓸 수 있게: 윈도우·안드로이드가 받지 않는 글자를 바꾸고, 공백을 하나로,
/// 앞의 점(숨은 파일이 된다)과 끝의 점·공백을 지우고, 너무 길면 자른다.
pub fn clean_name(s: &str, fallback: &str, max: usize) -> String {
    let mapped: String = s
        .chars()
        .filter_map(|c| match c {
            '/' | '\\' | ':' | '|' | '<' | '>' => Some('-'),
            '"' => Some('\''),
            '?' | '*' => None,
            c if c.is_control() => Some(' '),
            c => Some(c),
        })
        .collect();
    let mut out = mapped.split_whitespace().collect::<Vec<_>>().join(" ");
    if out.chars().count() > max {
        out = out.chars().take(max).collect();
    }
    let out = out.trim_start_matches(['.', ' ']).trim_end_matches(['.', ' ']).to_string();
    let reserved = {
        let u = out.split('.').next().unwrap_or("").trim().to_ascii_uppercase();
        matches!(u.as_str(), "CON" | "PRN" | "AUX" | "NUL")
            || (u.len() == 4 && (u.starts_with("COM") || u.starts_with("LPT")) && u.as_bytes()[3].is_ascii_digit())
    };
    if out.is_empty() {
        clean_name(fallback, "_", max)
    } else if reserved {
        format!("{out}_")
    } else {
        out
    }
}

/// 받는다. 이미 제 이름으로 있는 곡은 건너뛴다(전에 받아 둔 것). 취소하면 [`Cancelled`].
pub fn run(archive: &Archive, plan: &Plan, cancel: &AtomicBool, mut progress: impl FnMut(Progress)) -> anyhow::Result<()> {
    fs::create_dir_all(&plan.dir).with_context(|| format!("폴더를 만들 수 없습니다: {}", plan.dir.display()))?;
    let mut p = Progress { files_done: 0, files_total: plan.files.len(), bytes_done: 0, bytes_total: plan.total_bytes() };
    progress(p);
    for f in &plan.files {
        let dest = plan.dir.join(&f.name);
        if dest.is_file() {
            p.files_done += 1;
            p.bytes_done += f.size.unwrap_or(0);
            progress(p);
            continue;
        }
        let part = plan.dir.join(format!(".sori-{:02}.{}", f.track, extension(&f.file)));
        let before = p.bytes_done;
        let url = archive.download_url(&plan.id, &f.file);
        let res = fetch(&archive.media, &url, &part, f, cancel, &mut |n| {
            p.bytes_done = before + n;
            progress(p);
        })
        .and_then(|got| {
            // 태그를 못 적어도 소리는 온전하다. 곡은 남기고 기록만 한다.
            if let Err(e) = write_tags(&part, plan, f) {
                tracing::warn!(file = f.name, "태그를 적지 못했습니다: {e:#}");
            }
            fs::rename(&part, &dest).map_err(|e| DiskError(e).into()).map(|()| got)
        });
        match res {
            Ok(got) => {
                p.files_done += 1;
                p.bytes_done = before + f.size.unwrap_or(got);
                progress(p);
            }
            Err(e) => {
                let _ = fs::remove_file(&part);
                if e.is::<Cancelled>() {
                    return Err(e);
                }
                return Err(e.context(format!("{}을(를) 받지 못했습니다", f.title)));
            }
        }
    }
    Ok(())
}

struct Partial {
    out: File,
    hash: Sha1,
    done: u64,
}

/// 한 파일을 받는다. 끊기면 받은 데부터 이어 받고, 다 받으면 크기와 SHA-1을 맞춰 본다.
fn fetch(agent: &ureq::Agent, url: &str, part: &Path, f: &PlanFile, cancel: &AtomicBool, on: &mut dyn FnMut(u64)) -> anyhow::Result<u64> {
    let out = File::create(part).map_err(DiskError)?;
    let mut st = Partial { out, hash: Sha1::new(), done: 0 };
    let mut buf = vec![0u8; 256 << 10];
    let mut stalls = 0;
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Err(Cancelled.into());
        }
        let start = st.done;
        match attempt(agent, url, &mut st, f.size, cancel, &mut buf, on) {
            Ok(()) => break,
            Err(e) if e.is::<Cancelled>() || e.is::<DiskError>() => return Err(e),
            Err(e) => {
                stalls = if st.done > start { 0 } else { stalls + 1 };
                if stalls >= MAX_STALLS {
                    return Err(e);
                }
                tracing::info!(url, done = st.done, "끊겨서 이어 받습니다: {e:#}");
                // 진척이 없을 때만 조금씩 더 기다린다(0.5, 1, 2, 4초).
                let wait = if stalls == 0 { Duration::ZERO } else { Duration::from_millis(500 << (stalls - 1)) };
                let until = std::time::Instant::now() + wait;
                while std::time::Instant::now() < until {
                    if cancel.load(Ordering::Relaxed) {
                        return Err(Cancelled.into());
                    }
                    std::thread::sleep(Duration::from_millis(50));
                }
            }
        }
    }
    st.out.sync_all().map_err(DiskError)?;
    if let Some(size) = f.size
        && st.done != size
    {
        bail!("받은 크기가 다릅니다({} / {size}바이트)", st.done);
    }
    if let Some(want) = &f.sha1 {
        let got: String = st.hash.finalize().iter().map(|b| format!("{b:02x}")).collect();
        if !got.eq_ignore_ascii_case(want) {
            bail!("받은 파일이 손상됐습니다(SHA-1이 다릅니다)");
        }
    }
    Ok(st.done)
}

/// 한 번 연결해 받을 수 있는 데까지 받는다. 크기를 알면 그만큼 받기 전에 끝나는 것은 끊긴 것이다.
fn attempt(
    agent: &ureq::Agent,
    url: &str,
    st: &mut Partial,
    size: Option<u64>,
    cancel: &AtomicBool,
    buf: &mut [u8],
    on: &mut dyn FnMut(u64),
) -> anyhow::Result<()> {
    if size.is_some_and(|s| st.done >= s) {
        return Ok(());
    }
    let (res, _) = http::get_from(agent, url, st.done)?;
    if st.done > 0 {
        let resumed = res.status().as_u16() == 206 && http::content_range(res.headers()).is_some_and(|(start, _)| start == st.done);
        if !resumed {
            // 이어 받기를 받아 주지 않으면 처음부터
            st.out.set_len(0).map_err(DiskError)?;
            st.out.seek(SeekFrom::Start(0)).map_err(DiskError)?;
            st.hash = Sha1::new();
            st.done = 0;
        }
    }
    let mut body = res.into_body().into_reader();
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Err(Cancelled.into());
        }
        let n = body.read(buf)?;
        if n == 0 {
            if size.is_some_and(|s| st.done < s) {
                bail!("연결이 끊겼습니다");
            }
            return Ok(());
        }
        st.out.write_all(&buf[..n]).map_err(DiskError)?;
        st.hash.update(&buf[..n]);
        st.done += n as u64;
        on(st.done);
        if size.is_some_and(|s| st.done > s) {
            bail!("알려진 크기보다 큽니다");
        }
    }
}

/// 라이브러리가 공연 하나를 한 앨범으로 묶도록 적는다. 녹음한 사람이 적어 둔 다른 태그는 둔다.
fn write_tags(path: &Path, plan: &Plan, f: &PlanFile) -> anyhow::Result<()> {
    use lofty::config::WriteOptions;
    use lofty::prelude::*;
    use lofty::tag::Tag;
    use lofty::tag::items::Timestamp;

    let mut tagged = lofty::read_from_path(path)?;
    if tagged.primary_tag().is_none() {
        let kind = tagged.primary_tag_type();
        tagged.insert_tag(Tag::new(kind));
    }
    let Some(tag) = tagged.primary_tag_mut() else { bail!("태그를 붙일 수 없는 형식입니다") };
    tag.set_title(f.title.trim().to_string());
    tag.set_artist(plan.band.clone());
    tag.insert_text(ItemKey::AlbumArtist, plan.band.clone());
    tag.set_album(plan.album.clone());
    tag.set_track(f.track);
    tag.set_track_total(plan.files.len() as u32);
    // 장마다 다시 세던 번호를 이어 세므로 장 번호는 지운다.
    tag.remove_disk();
    tag.remove_disk_total();
    if let Some(ts) = plan.date.as_deref().and_then(|d| d.parse::<Timestamp>().ok()) {
        tag.set_date(ts);
    }
    if tag.genre().is_none() {
        tag.set_genre("Live".into());
    }
    if tag.comment().is_none() {
        tag.set_comment(format!("https://archive.org/details/{}", plan.id));
    }
    tag.save_to_path(path, WriteOptions::default())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::live::{LiveShow, LiveTrack, Quality};

    fn show(id: &str, venue: Option<&str>, titles: &[&str]) -> LiveShowDetail {
        LiveShowDetail {
            show: LiveShow {
                id: id.into(),
                title: "Band Live at Somewhere".into(),
                band: "King Gizzard & The Lizard Wizard".into(),
                date: Some("2023-06-07".into()),
                venue: venue.map(Into::into),
                city: Some("Morrison, CO".into()),
                downloads: None,
                quality: Quality::Flac24,
            },
            notes: None,
            source: None,
            lineage: None,
            taper: None,
            tracks: titles
                .iter()
                .enumerate()
                .map(|(i, t)| LiveTrack {
                    file: format!("d1t{:02}.flac", i + 1),
                    title: t.to_string(),
                    track_no: None,
                    duration: 1.0,
                    format: crate::live::preferred_format(1, &serde_json::Value::Null),
                    url: String::new(),
                    size: Some(10),
                    sha1: None,
                })
                .collect(),
            saveable: true,
            save_block: None,
        }
    }

    #[test]
    fn names_are_safe_on_every_system() {
        assert_eq!(clean_name("AC/DC: Live?", "x", 90), "AC-DC- Live");
        assert_eq!(clean_name("  Scarlet Begonias > Fire on the Mountain  ", "x", 90), "Scarlet Begonias - Fire on the Mountain");
        assert_eq!(clean_name("...And Justice", "x", 90), "And Justice", "점으로 시작하면 숨은 파일이 된다");
        assert_eq!(clean_name("Encore.", "x", 90), "Encore");
        assert_eq!(clean_name("con", "x", 90), "con_");
        assert_eq!(clean_name("COM1.flac", "x", 90), "COM1.flac_");
        assert_eq!(clean_name("Com10", "x", 90), "Com10");
        assert_eq!(clean_name("??", "Track 3", 90), "Track 3");
        assert_eq!(clean_name("a\tb\nc", "x", 90), "a b c");
        assert_eq!(clean_name(&"가".repeat(200), "x", 90).chars().count(), 90);
    }

    #[test]
    fn plans_folders_and_names() {
        let root = Path::new("/m/Live Archive");
        let s = show("kglw1", Some("Red Rocks Amphitheatre"), &["Mars For The Rich", "Hate Dancin'", "Set 2: Jam?"]);
        let p = plan(&s, root, |_| None).unwrap();
        assert_eq!(p.album, "2023-06-07 Red Rocks Amphitheatre");
        assert_eq!(p.dir, root.join("King Gizzard & The Lizard Wizard").join("2023-06-07 Red Rocks Amphitheatre"));
        assert_eq!(
            p.files.iter().map(|f| f.name.as_str()).collect::<Vec<_>>(),
            ["01 Mars For The Rich.flac", "02 Hate Dancin'.flac", "03 Set 2- Jam.flac"]
        );
        assert_eq!(p.files[2].title, "Set 2: Jam?", "태그에는 원래 제목");
        assert_eq!(p.total_bytes(), Some(30));

        // 공연장을 모르면 도시
        assert_eq!(plan(&show("x", None, &["a"]), root, |_| None).unwrap().album, "2023-06-07 Morrison, CO");

        // 같은 날 같은 곳의 다른 녹음은 (2)
        let taken = root.join("King Gizzard & The Lizard Wizard").join("2023-06-07 Red Rocks Amphitheatre");
        let p2 = plan(&s, root, |d| (d == taken).then(|| "other".to_string())).unwrap();
        assert_eq!(p2.album, "2023-06-07 Red Rocks Amphitheatre (2)");
        // 내가 전에 저장한 곳이면 그대로(이어 받기)
        let p3 = plan(&s, root, |d| (d == taken).then(|| "kglw1".to_string())).unwrap();
        assert_eq!(p3.dir, taken);

        // 100곡이 넘으면 세 자리
        let many: Vec<String> = (0..120).map(|i| format!("t{i}")).collect();
        let refs: Vec<&str> = many.iter().map(String::as_str).collect();
        assert_eq!(plan(&show("x", Some("v"), &refs), root, |_| None).unwrap().files[0].name, "001 t0.flac");
    }

    #[test]
    fn refuses_shows_the_artist_did_not_share() {
        let mut s = show("x", Some("v"), &["a"]);
        s.saveable = false;
        s.save_block = Some(crate::live::STREAM_ONLY.into());
        let e = plan(&s, Path::new("/m"), |_| None).unwrap_err().to_string();
        assert!(e.contains("듣기만"), "{e}");
    }

    #[test]
    fn music_folder_inside_storage_root() {
        let d = tempfile::tempdir().unwrap();
        assert_eq!(root(d.path()), d.path().join(FOLDER));
        std::fs::create_dir(d.path().join("Music")).unwrap();
        assert_eq!(root(d.path()), d.path().join("Music").join(FOLDER));
    }
}
