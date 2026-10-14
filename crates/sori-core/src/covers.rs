//! 앨범 표지.
//!
//! 폴더의 cover.jpg 같은 파일을 먼저 보고, 없으면 첫 곡에 박힌 그림을 꺼낸다. 원본이
//! 3000px짜리 PNG여도 그리드에는 480px, 전체 화면에는 1400px JPEG를 보낸다. 앨범
//! 수백 장이 한 화면에 뜨는 곳이라 원본을 그대로 보내면 스크롤이 멎는다.

use std::collections::{BTreeMap, HashMap};
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use image::codecs::jpeg::JpegEncoder;
use image::imageops::FilterType;
use image::{DynamicImage, ImageDecoder, ImageReader};
use lofty::config::ParseOptions;
use lofty::picture::PictureType;
use lofty::prelude::*;
use lofty::probe::Probe;
use parking_lot::Mutex;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::ids::make_id;
use crate::model::StoredTrack;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct CoverEntry {
    /// 어디서 왔는지와 그 파일의 수정 시각. 같으면 다시 만들지 않는다.
    pub sig: String,
    /// 비어 있으면 표지가 없다.
    pub v: String,
    pub color: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CoverSize {
    Sm,
    Lg,
}

impl CoverSize {
    pub fn px(self) -> u32 {
        match self {
            CoverSize::Sm => 480,
            CoverSize::Lg => 1400,
        }
    }
    pub fn key(self) -> &'static str {
        match self {
            CoverSize::Sm => "sm",
            CoverSize::Lg => "lg",
        }
    }
    pub fn parse(s: &str) -> CoverSize {
        if s == "lg" { CoverSize::Lg } else { CoverSize::Sm }
    }
}

pub fn cover_file(dir: &Path, album_id: &str, v: &str, size: CoverSize) -> PathBuf {
    dir.join(format!("{album_id}-{v}-{}.jpg", size.key()))
}

const PREFERRED: &[&str] = &["cover", "folder", "front", "album", "artwork", "albumart", "art"];
const IMAGE_EXT: &[&str] = &["jpg", "jpeg", "png", "webp", "gif", "bmp", "tif", "tiff"];

fn is_image(name: &str) -> bool {
    !name.starts_with('.') && name.rsplit_once('.').is_some_and(|(_, e)| IMAGE_EXT.contains(&e.to_ascii_lowercase().as_str()))
}

fn stem_lower(name: &str) -> String {
    name.rsplit_once('.').map(|(s, _)| s).unwrap_or(name).to_lowercase()
}

enum Source {
    File(PathBuf),
    Embedded(Vec<PathBuf>),
}

fn mtime_ms(p: &Path) -> Option<(i64, u64)> {
    let m = std::fs::metadata(p).ok()?;
    let t = m.modified().ok()?.duration_since(std::time::UNIX_EPOCH).ok()?.as_millis() as i64;
    Some((t, m.len()))
}

fn list_images(root: &Path, dir: &str, cache: &Mutex<HashMap<String, Vec<String>>>) -> Vec<String> {
    if let Some(v) = cache.lock().get(dir) {
        return v.clone();
    }
    let abs = if dir.is_empty() { root.to_path_buf() } else { root.join(dir) };
    let mut names: Vec<String> = std::fs::read_dir(abs)
        .map(|rd| {
            rd.flatten()
                .filter(|e| e.file_type().map(|t| t.is_file()).unwrap_or(false))
                .map(|e| e.file_name().to_string_lossy().to_string())
                .filter(|n| is_image(n))
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    cache.lock().insert(dir.to_string(), names.clone());
    names
}

fn find_source(root: &Path, tracks: &[&StoredTrack], cache: &Mutex<HashMap<String, Vec<String>>>) -> Option<(String, Source)> {
    let mut dirs: Vec<String> = Vec::new();
    for t in tracks {
        let d = t.path.rfind('/').map(|i| t.path[..i].to_string()).unwrap_or_default();
        if !dirs.contains(&d) {
            dirs.push(d.clone());
        }
        // CD1/CD2 폴더의 앨범은 표지가 부모 폴더에 있는 경우가 많다.
        let parent = crate::scan::album_dir(&d).to_string();
        if parent != d && !dirs.contains(&parent) {
            dirs.push(parent);
        }
    }
    for d in &dirs {
        let images = list_images(root, d, cache);
        let pick = PREFERRED
            .iter()
            .find_map(|p| images.iter().find(|n| stem_lower(n) == *p))
            .or_else(|| PREFERRED.iter().find_map(|p| images.iter().find(|n| n.to_lowercase().starts_with(p))))
            .or_else(|| if images.len() == 1 { images.first() } else { None });
        if let Some(name) = pick {
            let abs = if d.is_empty() { root.join(name) } else { root.join(d).join(name) };
            if let Some((m, s)) = mtime_ms(&abs) {
                return Some((format!("file:{d}/{name}:{m}:{s}"), Source::File(abs)));
            }
        }
    }
    // 폴더에 그림이 없으면 곡에 박힌 그림. 앞의 두 곡까지만 본다.
    let first: Vec<&&StoredTrack> = tracks.iter().take(2).collect();
    if first.is_empty() {
        return None;
    }
    let sig = format!("emb:{}", first.iter().map(|t| format!("{}:{}", t.path, t.mtime)).collect::<Vec<_>>().join("|"));
    Some((sig, Source::Embedded(first.iter().map(|t| root.join(&t.path)).collect())))
}

/// 곡에 박힌 그림 중 앞표지를, 없으면 아무거나 첫 번째를.
pub fn embedded_picture(path: &Path) -> Option<Vec<u8>> {
    let ext = crate::scan::ext_of(&path.to_string_lossy());
    let file = if ext == "dsf" || ext == "dff" {
        let info = crate::dsd::probe(path).ok()?;
        let wav = crate::dsd::id3_as_wav(path, &info)?;
        Probe::new(Cursor::new(wav)).set_file_type(lofty::file::FileType::Wav).read().ok()?
    } else {
        Probe::open(path).ok()?.guess_file_type().ok()?.options(ParseOptions::new().read_properties(false)).read().ok()?
    };
    let pics: Vec<_> = file.tags().iter().flat_map(|t| t.pictures().iter()).collect();
    let pic = pics.iter().find(|p| p.pic_type() == PictureType::CoverFront).or(pics.first())?;
    (!pic.data().is_empty()).then(|| pic.data().to_vec())
}

fn load(src: &Source) -> Option<Vec<u8>> {
    match src {
        Source::File(p) => std::fs::read(p).ok(),
        Source::Embedded(paths) => paths.iter().find_map(|p| embedded_picture(p)),
    }
}

/// 사진의 EXIF 방향까지 반영해서 연다.
pub fn decode_image(bytes: &[u8]) -> Option<DynamicImage> {
    let reader = ImageReader::new(Cursor::new(bytes)).with_guessed_format().ok()?;
    let mut decoder = reader.into_decoder().ok()?;
    let orientation = decoder.orientation().ok();
    let mut img = DynamicImage::from_decoder(decoder).ok()?;
    if let Some(o) = orientation {
        img.apply_orientation(o);
    }
    Some(img)
}

fn square(img: &DynamicImage, px: u32, enlarge: bool) -> DynamicImage {
    let side = img.width().min(img.height());
    let target = if enlarge { px } else { px.min(side) };
    img.resize_to_fill(target, target, FilterType::Lanczos3)
}

fn write_jpeg(img: &DynamicImage, path: &Path, quality: u8) -> anyhow::Result<()> {
    let rgb = img.to_rgb8();
    let mut out = Vec::with_capacity(256 * 1024);
    JpegEncoder::new_with_quality(&mut out, quality).encode_image(&rgb)?;
    crate::config::write_atomic(path, &out)?;
    Ok(())
}

/// 4096칸(채널마다 16단계) 색 히스토그램에서 가장 많은 칸의 평균 색.
pub fn dominant_color(img: &DynamicImage) -> String {
    let small = img.resize_to_fill(64, 64, FilterType::Triangle).to_rgb8();
    let mut bins = vec![(0u32, 0u64, 0u64, 0u64); 4096];
    for p in small.pixels() {
        let [r, g, b] = p.0;
        let k = ((r as usize >> 4) << 8) | ((g as usize >> 4) << 4) | (b as usize >> 4);
        let e = &mut bins[k];
        e.0 += 1;
        e.1 += r as u64;
        e.2 += g as u64;
        e.3 += b as u64;
    }
    let (n, r, g, b) = bins.into_iter().max_by_key(|e| e.0).unwrap_or((1, 0, 0, 0));
    let n = n.max(1) as u64;
    format!("#{:02x}{:02x}{:02x}", r / n, g / n, b / n)
}

/// 원본 바이트 → 두 크기의 표지와 대표 색.
pub fn render_cover(bytes: &[u8], dir: &Path, album_id: &str, v: &str) -> anyhow::Result<String> {
    let img = decode_image(bytes).ok_or_else(|| anyhow::anyhow!("그림을 열 수 없습니다"))?;
    write_jpeg(&square(&img, CoverSize::Sm.px(), true), &cover_file(dir, album_id, v, CoverSize::Sm), 84)?;
    write_jpeg(&square(&img, CoverSize::Lg.px(), false), &cover_file(dir, album_id, v, CoverSize::Lg), 90)?;
    Ok(dominant_color(&img))
}

pub fn update_covers(
    root: &Path,
    dir: &Path,
    tracks: &[StoredTrack],
    prev: &BTreeMap<String, CoverEntry>,
    report: &(dyn Fn(usize, usize) + Sync),
) -> BTreeMap<String, CoverEntry> {
    let _ = std::fs::create_dir_all(dir);
    let mut by_album: BTreeMap<&str, Vec<&StoredTrack>> = BTreeMap::new();
    for t in tracks {
        by_album.entry(t.album_id.as_str()).or_default().push(t);
    }
    for list in by_album.values_mut() {
        list.sort_by(|a, b| {
            a.disc_no
                .unwrap_or(1)
                .cmp(&b.disc_no.unwrap_or(1))
                .then(a.track_no.unwrap_or(0).cmp(&b.track_no.unwrap_or(0)))
                .then(a.path.cmp(&b.path))
        });
    }
    let total = by_album.len();
    report(0, total);
    let cache = Mutex::new(HashMap::new());
    let done = AtomicUsize::new(0);
    let result: BTreeMap<String, CoverEntry> = by_album
        .par_iter()
        .filter_map(|(album_id, list)| {
            let out = (|| {
                let (sig, src) = find_source(root, list, &cache)?;
                if let Some(before) = prev.get(*album_id)
                    && before.sig == sig
                    && (before.v.is_empty() || cover_file(dir, album_id, &before.v, CoverSize::Sm).exists())
                {
                    return Some(before.clone());
                }
                let Some(bytes) = load(&src) else {
                    return Some(CoverEntry { sig, v: String::new(), color: String::new() });
                };
                let v = make_id(&sig)[..8].to_string();
                // 깨진 그림이면 표지 없는 앨범으로 둔다.
                let color = render_cover(&bytes, dir, album_id, &v).ok()?;
                Some(CoverEntry { sig, v, color })
            })();
            let n = done.fetch_add(1, Ordering::Relaxed) + 1;
            if n.is_multiple_of(10) {
                report(n, total);
            }
            out.map(|e| (album_id.to_string(), e))
        })
        .collect();
    report(total, total);

    // 이제 아무도 가리키지 않는 표지 파일을 지운다.
    let mut keep = std::collections::HashSet::new();
    for (id, e) in &result {
        if !e.v.is_empty() {
            for s in [CoverSize::Sm, CoverSize::Lg] {
                keep.insert(cover_file(dir, id, &e.v, s).file_name().unwrap().to_string_lossy().to_string());
            }
        }
    }
    if let Ok(rd) = std::fs::read_dir(dir) {
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            if name.ends_with(".jpg") && !keep.contains(&name) {
                let _ = std::fs::remove_file(e.path());
            }
        }
    }
    result
}
