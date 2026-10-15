//! 음악 폴더를 읽어 색인을 만든다.
//!
//! 두 번째 스캔부터는 파일의 수정 시각과 크기만 보고 바뀐 파일만 태그를 다시 읽는다.
//! 수만 곡짜리 라이브러리도 다시 켤 때마다 몇 초면 끝나는 이유다.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use lofty::config::ParseOptions;
use lofty::file::FileType;
use lofty::prelude::*;
use lofty::probe::Probe;
use lofty::tag::Tag;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use walkdir::WalkDir;

use crate::covers::{CoverEntry, update_covers};
use crate::dsd;
use crate::formats::{FormatFacts, is_audio_ext, to_audio_format};
use crate::ids::{make_id, norm};
use crate::model::{ReplayGain, ScanPhase, StoredTrack, now_ms};

/// 색인 형식이 바뀌면 올린다. 올리면 처음부터 다시 스캔한다.
pub const INDEX_VERSION: u32 = 102;

pub const UNKNOWN_ARTIST: &str = "알 수 없는 아티스트";
pub const UNKNOWN_ALBUM: &str = "알 수 없는 앨범";

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexFile {
    pub version: u32,
    pub root: String,
    pub scanned_at: i64,
    pub tracks: Vec<StoredTrack>,
    /// 앨범 ID → 표지. `v`가 비어 있으면 "찾아봤는데 없었다".
    pub covers: BTreeMap<String, CoverEntry>,
}

pub struct ScanResult {
    pub index: IndexFile,
    pub added: usize,
    pub removed: usize,
    pub failed: usize,
}

#[derive(Clone, Debug)]
pub struct FileEntry {
    pub abs: PathBuf,
    pub rel: String,
    pub mtime: i64,
    pub size: u64,
    pub born: i64,
}

fn millis(t: std::io::Result<std::time::SystemTime>) -> Option<i64> {
    t.ok()?.duration_since(std::time::UNIX_EPOCH).ok().map(|d| d.as_millis() as i64)
}

fn hidden(name: &std::ffi::OsStr) -> bool {
    let s = name.to_string_lossy();
    // .DS_Store, 맥의 ._xxx.flac, 시놀로지의 @eaDir
    s.starts_with('.') || s.starts_with('@')
}

pub fn ext_of(p: &str) -> String {
    Path::new(p).extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default()
}

/// 숨김 파일은 건너뛴다. 심볼릭 링크는 따라가되 고리는 끊는다.
pub fn list_audio_files(root: &Path) -> Vec<FileEntry> {
    let mut out = Vec::new();
    let walker = WalkDir::new(root).follow_links(true).into_iter().filter_entry(|e| e.depth() == 0 || !hidden(e.file_name()));
    for entry in walker.flatten() {
        if !entry.file_type().is_file() {
            continue;
        }
        let Ok(rel) = entry.path().strip_prefix(root) else { continue };
        let rel = rel.components().map(|c| c.as_os_str().to_string_lossy()).collect::<Vec<_>>().join("/");
        if !is_audio_ext(&ext_of(&rel)) {
            continue;
        }
        let Ok(meta) = entry.metadata() else { continue };
        let mtime = millis(meta.modified()).unwrap_or(0);
        out.push(FileEntry {
            abs: entry.path().to_path_buf(),
            rel,
            mtime,
            size: meta.len(),
            born: millis(meta.created()).unwrap_or(mtime),
        });
    }
    out
}

/// 음악을 찾을 때 들어가지 않는 폴더(앱 데이터, 휴지통 …)
fn skip_dir(name: &std::ffi::OsStr) -> bool {
    ["Android", "AppData", "Library", "node_modules", "$RECYCLE.BIN", "System Volume Information"].iter().any(|s| name == *s)
}

/// 폴더 안의 곡 수를 `cap`까지 센다(`deadline`이 지나면 거기까지). 음악 폴더를 고를 때 보여 준다.
pub fn count_audio(dir: &Path, cap: usize, deadline: std::time::Instant) -> usize {
    let walker = WalkDir::new(dir)
        .max_depth(8)
        .into_iter()
        .filter_entry(|e| e.depth() == 0 || !(hidden(e.file_name()) || (e.depth() == 1 && skip_dir(e.file_name()))));
    let mut n = 0;
    for entry in walker.flatten() {
        if n >= cap || std::time::Instant::now() >= deadline {
            break;
        }
        if entry.file_type().is_file() && is_audio_ext(&ext_of(&entry.file_name().to_string_lossy())) {
            n += 1;
        }
    }
    n
}

/// 음악이 있을 만한 폴더들: `roots` 바로 아래의 폴더마다 곡 수를 세어, 곡이 있는 것만 많은 순서로.
/// 저장공간 전체(루트 자체)도 함께 낸다. 휴대폰에는 폴더 고르기 창이 없어서 이것으로 고른다.
pub fn suggest_music_dirs(roots: &[PathBuf], cap: usize, budget: std::time::Duration) -> Vec<(PathBuf, usize)> {
    let deadline = std::time::Instant::now() + budget;
    let mut out = Vec::new();
    for root in roots {
        let Ok(rd) = std::fs::read_dir(root) else { continue };
        let mut subs: Vec<PathBuf> = rd
            .flatten()
            .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()) && !hidden(&e.file_name()) && !skip_dir(&e.file_name()))
            .map(|e| e.path())
            .collect();
        subs.sort();
        let mut total = 0;
        for d in subs {
            let n = count_audio(&d, cap, deadline);
            total += n;
            if n > 0 {
                out.push((d, n));
            }
        }
        if total > 0 {
            out.push((root.clone(), total.min(cap)));
        }
    }
    out.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    out
}

fn is_disc_dir(name: &str) -> Option<u32> {
    // /^(cd|disc|disk|디스크)\s*[-_.]?\s*(\d+)\b/i
    let lower = name.to_lowercase();
    let rest = ["cd", "disc", "disk", "디스크"].iter().find_map(|p| lower.strip_prefix(p))?;
    let rest = rest.trim_start();
    let rest = rest.strip_prefix(['-', '_', '.']).unwrap_or(rest).trim_start();
    let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
    if digits.is_empty() {
        return None;
    }
    let after = rest[digits.len()..].chars().next();
    if after.is_some_and(|c| c.is_alphanumeric() || c == '_') {
        return None;
    }
    digits.parse().ok()
}

fn parent(rel: &str) -> &str {
    rel.rfind('/').map(|i| &rel[..i]).unwrap_or("")
}

fn base(rel: &str) -> &str {
    rel.rfind('/').map(|i| &rel[i + 1..]).unwrap_or(rel)
}

/// "앨범/CD2" 같은 디스크 폴더는 앨범 폴더 하나로 친다.
pub fn album_dir(rel_dir: &str) -> &str {
    if rel_dir.is_empty() || rel_dir == "." {
        return "";
    }
    if is_disc_dir(base(rel_dir)).is_some() { parent(rel_dir) } else { rel_dir }
}

fn disc_from_dir(rel_dir: &str) -> Option<u32> {
    is_disc_dir(base(rel_dir))
}

fn clean(s: &str) -> String {
    s.replace('\0', "").trim().to_string()
}

fn uniq(xs: impl IntoIterator<Item = String>) -> Vec<String> {
    let mut seen = HashSet::new();
    xs.into_iter().filter(|x| !x.is_empty() && seen.insert(norm(x))).collect()
}

/// 4자리 연도(1800–2099)를 낱말 경계 안에서 찾는다. "2020-05-01" → 2020.
pub fn parse_year(s: &str) -> Option<u32> {
    s.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
        .find(|w| {
            w.len() == 4 && w.bytes().all(|b| b.is_ascii_digit()) && (w.starts_with("18") || w.starts_with("19") || w.starts_with("20"))
        })
        .and_then(|w| w.parse().ok())
}

/// 태그가 없을 때 "01 - Song_Name" → 번호 1, 제목 "Song Name".
pub fn from_file_name(stem: &str) -> (Option<u32>, String) {
    let digits: String = stem.chars().take_while(|c| c.is_ascii_digit()).collect();
    let fallback = || stem.replace('_', " ").trim().to_string();
    if (1..=3).contains(&digits.len()) {
        let rest = &stem[digits.len()..];
        let trimmed = rest.trim_start();
        let after_sep = trimmed.strip_prefix(['-', '.', '_', ')']).map(str::trim_start);
        let title = match after_sep {
            Some(t) if !t.is_empty() => Some(t),
            // 구분자 없이 공백만 있는 경우: "01 Song"
            None if trimmed.len() < rest.len() && !trimmed.is_empty() => Some(trimmed),
            _ => None,
        };
        if let Some(t) = title {
            let t = t.replace('_', " ").trim().to_string();
            return (digits.parse().ok(), if t.is_empty() { fallback() } else { t });
        }
    }
    let t = fallback();
    (None, if t.is_empty() { stem.to_string() } else { t })
}

/// "-6.53 dB" → -6.53
fn parse_db(s: &str) -> Option<f64> {
    let t = s.trim().trim_end_matches(|c: char| c.is_alphabetic() || c.is_whitespace());
    t.trim().replace(',', ".").parse::<f64>().ok().filter(|v| v.is_finite())
}

#[derive(Default)]
struct Tags {
    title: Option<String>,
    artists: Vec<String>,
    album: Option<String>,
    album_artist: Option<String>,
    track: Option<u32>,
    disc: Option<u32>,
    year: Option<u32>,
    genres: Vec<String>,
    rg: ReplayGain,
}

fn first(tags: &[&Tag], f: impl Fn(&Tag) -> Option<String>) -> Option<String> {
    tags.iter().find_map(|t| f(t).map(|s| clean(&s)).filter(|s| !s.is_empty()))
}

fn collect_tags(tags: &[&Tag]) -> Tags {
    let mut t = Tags {
        title: first(tags, |t| t.title().map(|s| s.into_owned())),
        album: first(tags, |t| t.album().map(|s| s.into_owned())),
        album_artist: first(tags, |t| t.get_string(ItemKey::AlbumArtist).map(str::to_string)),
        ..Default::default()
    };
    // 여러 값이 들어 있는 아티스트(ARTIST가 두 번, ID3v2.4의 \0 구분)는 나눠서 본다.
    for tag in tags {
        let names: Vec<String> = tag
            .get_strings(ItemKey::TrackArtist)
            .chain(tag.get_strings(ItemKey::TrackArtists))
            .flat_map(|s| s.split('\0').map(clean).collect::<Vec<_>>())
            .collect();
        if !names.is_empty() {
            t.artists = uniq(names);
            break;
        }
    }
    for tag in tags {
        let g: Vec<String> = tag.get_strings(ItemKey::Genre).flat_map(|s| s.split([';', '\0']).map(clean).collect::<Vec<_>>()).collect();
        if !g.is_empty() {
            t.genres = uniq(g);
            break;
        }
    }
    t.track = tags.iter().find_map(|x| x.track()).filter(|&n| n > 0);
    t.disc = tags.iter().find_map(|x| x.disk()).filter(|&n| n > 0);
    t.year = tags.iter().find_map(|x| x.date().map(|d| d.year as u32)).filter(|y| (1000..3000).contains(y)).or_else(|| {
        tags.iter().find_map(|x| {
            [ItemKey::Year, ItemKey::RecordingDate, ItemKey::ReleaseDate, ItemKey::OriginalReleaseDate]
                .into_iter()
                .find_map(|k| x.get_string(k).and_then(parse_year))
        })
    });
    let db = |k: ItemKey| tags.iter().find_map(|x| x.get_string(k).and_then(parse_db));
    t.rg = ReplayGain {
        tg: db(ItemKey::ReplayGainTrackGain),
        tp: db(ItemKey::ReplayGainTrackPeak),
        ag: db(ItemKey::ReplayGainAlbumGain),
        ap: db(ItemKey::ReplayGainAlbumPeak),
    };
    t
}

fn codec_of(ft: FileType, path: &Path) -> &'static str {
    match ft {
        FileType::Flac => "FLAC",
        FileType::Mpeg => "MP3",
        FileType::Wav => "WAV",
        FileType::Aiff => "AIFF",
        FileType::Opus => "Opus",
        FileType::Vorbis => "Vorbis",
        FileType::Speex => "Speex",
        FileType::Ape => "APE",
        FileType::WavPack => "WavPack",
        FileType::Aac => "AAC",
        FileType::Mpc => "Musepack",
        FileType::Mp4 => {
            let codec = std::fs::File::open(path).ok().and_then(|mut f| {
                lofty::mp4::Mp4File::read_from(&mut f, ParseOptions::new().read_tags(false)).ok().and_then(|m| m.properties().codec())
            });
            match codec {
                Some(lofty::mp4::Mp4Codec::ALAC) => "ALAC",
                Some(lofty::mp4::Mp4Codec::FLAC) => "FLAC",
                Some(lofty::mp4::Mp4Codec::MP3) => "MP3",
                Some(lofty::mp4::Mp4Codec::AAC) => "AAC",
                // 태그 읽는 쪽이 모르는 코덱(돌비 E-AC-3·AC-4, Opus …): 디코더 쪽에 진짜 이름을 묻는다.
                _ => crate::decode::probe_container(path).map(|i| i.codec).unwrap_or("AAC"),
            }
        }
        _ => "PCM",
    }
}

struct Probed {
    facts: FormatFacts,
    duration: f64,
    tags: Tags,
}

fn probe_file(path: &Path, ext: &str) -> Option<Probed> {
    if ext == "dsf" || ext == "dff" {
        let info = dsd::probe(path).ok()?;
        let tags = dsd::id3_as_wav(path, &info)
            .and_then(|wav| {
                Probe::new(Cursor::new(wav)).set_file_type(FileType::Wav).options(ParseOptions::new().read_cover_art(false)).read().ok()
            })
            .map(|f| collect_tags(&f.tags().iter().collect::<Vec<_>>()))
            .unwrap_or_default();
        return Some(Probed {
            facts: FormatFacts {
                codec: "DSD",
                sample_rate: Some(info.rate),
                bit_depth: None,
                bitrate: Some((info.rate as u64 * info.channels as u64).min(u32::MAX as u64) as u32),
                channels: Some(info.channels),
            },
            duration: info.duration(),
            tags,
        });
    }
    if crate::formats::needs_container_probe(ext) {
        // 태그는 못 읽으니 이름은 파일 이름에서, 형식과 길이는 컨테이너에서.
        let info = crate::decode::probe_container(path).ok()?;
        return Some(Probed {
            facts: FormatFacts {
                codec: info.codec,
                sample_rate: info.sample_rate,
                bit_depth: info.bits,
                bitrate: (info.duration > 0.0)
                    .then(|| std::fs::metadata(path).ok().map(|m| (m.len() as f64 * 8.0 / info.duration) as u32))
                    .flatten(),
                channels: info.channels,
            },
            duration: info.duration,
            tags: Tags::default(),
        });
    }
    let file = Probe::open(path).ok()?.guess_file_type().ok()?.options(ParseOptions::new().read_cover_art(false)).read().ok()?;
    let props = file.properties();
    let mut ordered: Vec<&Tag> = Vec::new();
    if let Some(p) = file.primary_tag() {
        ordered.push(p);
    }
    for t in file.tags() {
        if !ordered.iter().any(|o| std::ptr::eq(*o, t)) {
            ordered.push(t);
        }
    }
    let codec = codec_of(file.file_type(), path);
    Some(Probed {
        facts: FormatFacts {
            codec,
            sample_rate: props.sample_rate(),
            bit_depth: props.bit_depth().map(u32::from),
            bitrate: props.audio_bitrate().or(props.overall_bitrate()).map(|k| k * 1000),
            channels: props.channels().map(u32::from),
        },
        duration: props.duration().as_secs_f64(),
        tags: collect_tags(&ordered),
    })
}

pub fn read_track(f: &FileEntry, added_at: i64) -> Option<StoredTrack> {
    let ext = ext_of(&f.rel);
    let p = probe_file(&f.abs, &ext)?;
    let rel_dir = parent(&f.rel);
    let stem = base(&f.rel);
    let stem = stem.rfind('.').map(|i| &stem[..i]).unwrap_or(stem);
    let (file_no, file_title) = from_file_name(stem);
    let dir = album_dir(rel_dir);
    let t = p.tags;

    let artists = t.artists;
    let artist = if artists.is_empty() { crate::scan::UNKNOWN_ARTIST.to_string() } else { artists.join(", ") };
    let artist = if artists.len() == 1 { artists[0].clone() } else { artist };
    let album = t.album.clone().unwrap_or_else(|| if dir.is_empty() { UNKNOWN_ALBUM.to_string() } else { base(dir).to_string() });
    // 앨범 아티스트가 적혀 있으면 그걸로, 없으면 폴더로 묶는다. 폴더로 묶어야 태그 없는
    // 컴필레이션이 곡마다 다른 아티스트 때문에 열 조각으로 흩어지지 않는다.
    let key = match &t.album_artist {
        Some(aa) => format!("aa:{}|{}", norm(aa), norm(&album)),
        None => format!("dir:{}|{}", dir, norm(&album)),
    };
    Some(StoredTrack {
        id: make_id(&format!("track:{}", f.rel)),
        path: f.rel.clone(),
        mtime: f.mtime,
        size: f.size,
        title: t.title.unwrap_or(file_title),
        artists: if artists.is_empty() { vec![artist.clone()] } else { artists },
        artist,
        album,
        album_id: make_id(&format!("album:{key}")),
        album_artist_tag: t.album_artist,
        track_no: t.track.or(file_no),
        disc_no: t.disc.or_else(|| disc_from_dir(rel_dir)),
        year: t.year,
        genres: t.genres,
        duration: p.duration,
        format: to_audio_format(p.facts, &ext),
        added_at,
        rg: (!t.rg.is_empty()).then_some(t.rg),
    })
}

pub fn scan_library(
    root: &Path,
    covers_dir: &Path,
    prev: Option<&IndexFile>,
    report: &(dyn Fn(ScanPhase, usize, usize) + Sync),
) -> anyhow::Result<ScanResult> {
    if !root.is_dir() {
        anyhow::bail!("음악 폴더를 찾을 수 없습니다: {}", root.display());
    }
    let root_s = root.to_string_lossy().to_string();
    // 폴더를 바꿨거나 색인 형식이 바뀌었으면 처음부터.
    let usable = prev.filter(|p| p.root == root_s && p.version == INDEX_VERSION);
    let old: HashMap<&str, &StoredTrack> = usable.map(|p| p.tracks.iter().map(|t| (t.path.as_str(), t)).collect()).unwrap_or_default();

    report(ScanPhase::Listing, 0, 0);
    let files = list_audio_files(root);
    let total = files.len();
    report(ScanPhase::Reading, 0, total);

    let processed = AtomicUsize::new(0);
    let added = AtomicUsize::new(0);
    let failed = AtomicUsize::new(0);
    let mut tracks: Vec<StoredTrack> = files
        .par_iter()
        .filter_map(|f| {
            let o = old.get(f.rel.as_str());
            let t = match o {
                Some(o) if o.mtime == f.mtime && o.size == f.size => Some((*o).clone()),
                _ => {
                    let t = read_track(f, o.map(|o| o.added_at).unwrap_or(f.born));
                    if t.is_none() {
                        failed.fetch_add(1, Ordering::Relaxed);
                    } else if o.is_none() {
                        added.fetch_add(1, Ordering::Relaxed);
                    }
                    t
                }
            };
            let n = processed.fetch_add(1, Ordering::Relaxed) + 1;
            if n.is_multiple_of(25) {
                report(ScanPhase::Reading, n, total);
            }
            t
        })
        .collect();
    report(ScanPhase::Reading, total, total);

    let present: HashSet<&str> = files.iter().map(|f| f.rel.as_str()).collect();
    let removed = old.keys().filter(|p| !present.contains(*p)).count();
    tracks.sort_by(|a, b| a.path.cmp(&b.path));

    let empty = BTreeMap::new();
    let covers = update_covers(root, covers_dir, &tracks, usable.map(|p| &p.covers).unwrap_or(&empty), &|done, total| {
        report(ScanPhase::Covers, done, total)
    });

    Ok(ScanResult {
        index: IndexFile { version: INDEX_VERSION, root: root_s, scanned_at: now_ms(), tracks, covers },
        added: added.into_inner(),
        removed,
        failed: failed.into_inner(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_names() {
        assert_eq!(from_file_name("01 - Song_Name"), (Some(1), "Song Name".to_string()));
        assert_eq!(from_file_name("7. Intro"), (Some(7), "Intro".to_string()));
        assert_eq!(from_file_name("12 Hello"), (Some(12), "Hello".to_string()));
        assert_eq!(from_file_name("2001 A Space Odyssey"), (None, "2001 A Space Odyssey".to_string()));
        assert_eq!(from_file_name("Track"), (None, "Track".to_string()));
        assert_eq!(from_file_name("03"), (None, "03".to_string()));
    }

    #[test]
    fn disc_dirs() {
        assert_eq!(album_dir("Artist/Album/CD2"), "Artist/Album");
        assert_eq!(album_dir("Artist/Album/Disc 1"), "Artist/Album");
        assert_eq!(album_dir("Artist/Album/디스크 3"), "Artist/Album");
        assert_eq!(album_dir("Artist/CD Collection"), "Artist/CD Collection");
        assert_eq!(album_dir("Artist/Album"), "Artist/Album");
        assert_eq!(disc_from_dir("A/B/cd-02"), Some(2));
        assert_eq!(disc_from_dir("A/B/Discography"), None);
    }

    #[test]
    fn years_and_gains() {
        assert_eq!(parse_year("2020-05-01"), Some(2020));
        assert_eq!(parse_year("Recorded 1969"), Some(1969));
        assert_eq!(parse_year("x2020"), None);
        assert_eq!(parse_db("-6.53 dB"), Some(-6.53));
        assert_eq!(parse_db("+1.2dB"), Some(1.2));
        assert_eq!(parse_db("0.988"), Some(0.988));
    }
}

#[cfg(test)]
mod suggest_tests {
    use super::*;

    #[test]
    fn suggests_folders_with_music() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        for (p, body) in [
            ("Music/a/1.flac", ""),
            ("Music/a/2.mp3", ""),
            ("Download/x.mp3", ""),
            ("Download/doc.pdf", ""),
            ("DCIM/p.jpg", ""),
            (".hidden/z.flac", ""),
            ("Android/data/q.mp3", ""),
        ] {
            let f = root.join(p);
            std::fs::create_dir_all(f.parent().unwrap()).unwrap();
            std::fs::write(f, body).unwrap();
        }
        let got = suggest_music_dirs(&[root.to_path_buf()], 999, std::time::Duration::from_secs(5));
        let names: Vec<(String, usize)> =
            got.iter().map(|(p, n)| (p.strip_prefix(root).unwrap().to_string_lossy().to_string(), *n)).collect();
        assert_eq!(names, [("".to_string(), 3), ("Music".to_string(), 2), ("Download".to_string(), 1)]);
    }
}
