//! 색인(곡 목록) → 앨범·아티스트·장르.
//!
//! 저장하는 것은 곡뿐이고 나머지는 전부 여기서 다시 계산한다. 묶는 규칙을 고쳐도
//! 색인을 지우고 다시 스캔할 필요가 없다.

use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};

use serde::Serialize;

use crate::analysis::AnalysisFile;
use crate::collate::compare;
use crate::ids::{artist_id, genre_id, norm};
use crate::model::*;
use crate::quality::{better_format, tier_of};
use crate::scan::{IndexFile, UNKNOWN_ARTIST};

pub const VARIOUS: &str = "Various Artists";

#[derive(Default)]
pub struct Library {
    pub tracks: HashMap<String, StoredTrack>,
    pub views: HashMap<String, Track>,
    pub albums: HashMap<String, Album>,
    pub artists: HashMap<String, Artist>,
    pub genres: HashMap<String, Genre>,
    /// 앨범 아티스트 → 연도 순.
    pub album_list: Vec<String>,
    pub artist_list: Vec<String>,
    pub genre_list: Vec<String>,
    /// 아티스트 → 앨범 → 디스크 → 트랙 순.
    pub track_list: Vec<String>,
    pub summary: LibrarySummary,
    hay_albums: Vec<String>,
    hay_artists: Vec<String>,
    hay_tracks: Vec<String>,
}

/// 정렬할 때는 "The Beatles"를 B에 둔다.
pub fn sort_name(name: &str) -> &str {
    let lower = name.to_ascii_lowercase();
    if lower.starts_with("the ") { name[4..].trim_start() } else { name }
}

/// 아티스트 이름 순. 이름을 모르는 것은 맨 뒤로.
pub fn compare_artist(a: &str, b: &str) -> Ordering {
    let ua = (a == UNKNOWN_ARTIST || a == VARIOUS) as u8;
    let ub = (b == UNKNOWN_ARTIST || b == VARIOUS) as u8;
    ua.cmp(&ub).then_with(|| compare(sort_name(a), sort_name(b)))
}

fn most_common(xs: &[&str]) -> Option<String> {
    let mut counts: HashMap<&str, usize> = HashMap::new();
    let mut best: Option<&str> = None;
    let mut best_n = 0;
    for x in xs {
        let n = counts.entry(x).or_insert(0);
        *n += 1;
        if *n > best_n {
            best = Some(x);
            best_n = *n;
        }
    }
    best.map(str::to_string)
}

fn round1(x: f64) -> f64 {
    (x * 10.0).round() / 10.0
}

/// ReplayGain 기준은 -18 LUFS(89dB SPL). 게인 g면 곡의 라우드니스는 -18 - g.
pub fn from_replay_gain(gain: Option<f64>, peak_ratio: Option<f64>) -> Option<LoudnessValue> {
    let gain = gain.filter(|g| g.is_finite())?;
    let peak = match peak_ratio {
        Some(p) if p > 0.0 => 20.0 * p.log10(),
        _ => 0.0,
    };
    Some(LoudnessValue { lufs: round1(-18.0 - gain), peak: round1(peak) })
}

/// 앨범 라우드니스: 곡 길이로 가중한 에너지 평균. 정식 앨범 게이팅의 근사값이다.
pub fn album_loudness(tracks: &[(f64, f64, f64)]) -> Option<LoudnessValue> {
    let valid: Vec<_> = tracks.iter().filter(|(l, _, d)| l.is_finite() && *l > -69.0 && *d > 0.0).collect();
    if valid.is_empty() {
        return None;
    }
    let total: f64 = valid.iter().map(|t| t.2).sum();
    let energy = valid.iter().map(|(l, _, d)| d * 10f64.powf(l / 10.0)).sum::<f64>() / total;
    Some(LoudnessValue { lufs: round1(10.0 * energy.log10()), peak: valid.iter().map(|t| t.1).fold(f64::NEG_INFINITY, f64::max) })
}

fn track_order(a: &StoredTrack, b: &StoredTrack) -> Ordering {
    a.disc_no
        .unwrap_or(1)
        .cmp(&b.disc_no.unwrap_or(1))
        .then(a.track_no.unwrap_or(1_000_000).cmp(&b.track_no.unwrap_or(1_000_000)))
        .then_with(|| compare(&a.path, &b.path))
}

impl Library {
    pub fn build(index: &IndexFile, analysis: Option<&AnalysisFile>) -> Library {
        let tracks: HashMap<String, StoredTrack> = index.tracks.iter().map(|t| (t.id.clone(), t.clone())).collect();
        let mut groups: HashMap<&str, Vec<&StoredTrack>> = HashMap::new();
        for t in &index.tracks {
            groups.entry(t.album_id.as_str()).or_default().push(t);
        }
        for list in groups.values_mut() {
            list.sort_by(|a, b| track_order(a, b));
        }

        let mut albums: HashMap<String, Album> = HashMap::new();
        let mut artists: HashMap<String, Artist> = HashMap::new();
        fn get_artist<'a>(artists: &'a mut HashMap<String, Artist>, name: &str) -> &'a mut Artist {
            let id = artist_id(name);
            artists.entry(id.clone()).or_insert_with(|| Artist {
                id,
                name: name.to_string(),
                album_ids: vec![],
                appears_on: vec![],
                cover_album_id: None,
                cover: None,
            })
        }

        for (id, list) in &groups {
            let tag = list.iter().find_map(|t| t.album_artist_tag.clone());
            let track_artists: HashSet<String> = list.iter().map(|t| norm(&t.artist)).collect();
            let artist = tag.unwrap_or_else(|| if track_artists.len() == 1 { list[0].artist.clone() } else { VARIOUS.to_string() });
            let mut genre_counts: Vec<(String, usize)> = Vec::new();
            for t in list {
                for g in &t.genres {
                    match genre_counts.iter_mut().find(|(n, _)| n == g) {
                        Some(e) => e.1 += 1,
                        None => genre_counts.push((g.clone(), 1)),
                    }
                }
            }
            genre_counts.sort_by_key(|g| std::cmp::Reverse(g.1));
            let cover = index.covers.get(*id).filter(|c| !c.v.is_empty()).map(|c| CoverRef { v: c.v.clone(), color: c.color.clone() });
            let mut format = &list[0].format;
            for t in list.iter().skip(1) {
                format = better_format(format, &t.format);
            }
            let titles: Vec<&str> = list.iter().map(|t| t.album.as_str()).collect();
            let album = Album {
                id: id.to_string(),
                title: most_common(&titles).unwrap_or_else(|| list[0].album.clone()),
                artist_id: artist_id(&artist),
                artist: artist.clone(),
                year: list.iter().filter_map(|t| t.year).min(),
                genres: genre_counts.into_iter().take(3).map(|(g, _)| g).collect(),
                track_ids: list.iter().map(|t| t.id.clone()).collect(),
                disc_count: list.iter().map(|t| t.disc_no.unwrap_or(1)).collect::<HashSet<_>>().len() as u32,
                duration: list.iter().map(|t| t.duration).sum(),
                added_at: list.iter().map(|t| t.added_at).max().unwrap_or(0),
                cover,
                format: format.clone(),
            };
            get_artist(&mut artists, &artist).album_ids.push(id.to_string());
            albums.insert(id.to_string(), album);
        }

        // 라우드니스: 분석이 있으면 분석, 없으면 ReplayGain 태그. 앨범 값은 앨범의 분석된 곡들로.
        let mut track_loud: HashMap<&str, LoudnessValue> = HashMap::new();
        let mut album_loud: HashMap<&str, LoudnessValue> = HashMap::new();
        for (album_id, list) in &groups {
            let mut analyzed = Vec::new();
            for t in list {
                if let Some(a) = analysis.and_then(|a| a.tracks.get(&t.id)).filter(|a| a.m == t.mtime) {
                    track_loud.insert(t.id.as_str(), LoudnessValue { lufs: a.i, peak: a.p });
                    analyzed.push((a.i, a.p, t.duration));
                }
            }
            let al =
                album_loudness(&analyzed).or_else(|| list.iter().find_map(|t| t.rg.as_ref().and_then(|rg| from_replay_gain(rg.ag, rg.ap))));
            if let Some(al) = al {
                album_loud.insert(album_id, al);
            }
        }

        let mut views: HashMap<String, Track> = HashMap::new();
        for t in &index.tracks {
            let album = &albums[&t.album_id];
            let primary = t.artists.first().cloned().unwrap_or_else(|| t.artist.clone());
            let analyzed = track_loud.get(t.id.as_str()).copied();
            let tl = analyzed.or_else(|| t.rg.as_ref().and_then(|rg| from_replay_gain(rg.tg, rg.tp)));
            let al = album_loud.get(t.album_id.as_str()).copied();
            let loudness = (tl.is_some() || al.is_some()).then(|| Loudness {
                track: tl,
                album: al,
                source: if analyzed.is_some() { LoudnessSource::Analysis } else { LoudnessSource::Replaygain },
            });
            views.insert(
                t.id.clone(),
                Track {
                    id: t.id.clone(),
                    title: t.title.clone(),
                    artist: t.artist.clone(),
                    artist_id: artist_id(&primary),
                    artists: t.artists.clone(),
                    album: t.album.clone(),
                    album_id: t.album_id.clone(),
                    album_artist: album.artist.clone(),
                    album_artist_id: album.artist_id.clone(),
                    track_no: t.track_no,
                    disc_no: t.disc_no,
                    year: t.year,
                    genres: t.genres.clone(),
                    duration: t.duration,
                    format: t.format.clone(),
                    added_at: t.added_at,
                    cover: album.cover.clone(),
                    loudness,
                },
            );
            let album_artist = norm(&album.artist);
            for name in &t.artists {
                if norm(name) != album_artist {
                    get_artist(&mut artists, name).appears_on.push(t.id.clone());
                }
            }
            // 곡 아티스트가 "A feat. B"처럼 artists에 없더라도 링크가 끊기지 않게.
            if !t.artists.iter().any(|n| norm(n) == norm(&primary)) {
                get_artist(&mut artists, &primary);
            }
        }

        let mut album_list: Vec<String> = albums.keys().cloned().collect();
        album_list.sort_by(|a, b| {
            let (a, b) = (&albums[a], &albums[b]);
            compare_artist(&a.artist, &b.artist)
                .then(a.year.unwrap_or(0).cmp(&b.year.unwrap_or(0)))
                .then_with(|| compare(&a.title, &b.title))
                .then_with(|| a.id.cmp(&b.id))
        });
        let order: HashMap<&str, usize> = album_list.iter().enumerate().map(|(i, id)| (id.as_str(), i)).collect();

        for a in artists.values_mut() {
            a.album_ids.sort_by(|x, y| albums[y].year.unwrap_or(0).cmp(&albums[x].year.unwrap_or(0)));
            let cover_album = a
                .album_ids
                .iter()
                .map(|id| &albums[id])
                .find(|al| al.cover.is_some())
                .or_else(|| a.appears_on.iter().map(|id| &albums[&tracks[id].album_id]).find(|al| al.cover.is_some()));
            a.cover_album_id = cover_album.map(|al| al.id.clone());
            a.cover = cover_album.and_then(|al| al.cover.clone());
        }

        let various = norm(VARIOUS);
        let mut artist_list: Vec<String> = artists
            .values()
            .filter(|a| norm(&a.name) != various && (!a.album_ids.is_empty() || !a.appears_on.is_empty()))
            .map(|a| a.id.clone())
            .collect();
        artist_list.sort_by(|a, b| compare_artist(&artists[a].name, &artists[b].name).then_with(|| a.cmp(b)));

        let mut genres: HashMap<String, Genre> = HashMap::new();
        for id in &album_list {
            let al = &albums[id];
            for name in &al.genres {
                let gid = genre_id(name);
                let g =
                    genres.entry(gid.clone()).or_insert_with(|| Genre { id: gid, name: name.clone(), album_ids: vec![], covers: vec![] });
                g.album_ids.push(al.id.clone());
                if let Some(c) = &al.cover
                    && g.covers.len() < 4
                {
                    g.covers.push(GenreCover { album_id: al.id.clone(), cover: c.clone() });
                }
            }
        }
        let mut genre_list: Vec<String> = genres.keys().cloned().collect();
        genre_list.sort_by(|a, b| {
            let (a, b) = (&genres[a], &genres[b]);
            b.album_ids.len().cmp(&a.album_ids.len()).then_with(|| compare(&a.name, &b.name))
        });

        let mut track_list: Vec<String> = views.keys().cloned().collect();
        track_list.sort_by(|a, b| {
            let (a, b) = (&tracks[a], &tracks[b]);
            order[a.album_id.as_str()].cmp(&order[b.album_id.as_str()]).then_with(|| track_order(a, b))
        });

        let (mut hires, mut lossless) = (0, 0);
        for al in albums.values() {
            match tier_of(&al.format) {
                QualityTier::Hires => {
                    hires += 1;
                    lossless += 1;
                }
                QualityTier::Lossless => lossless += 1,
                QualityTier::Lossy => {}
            }
        }
        let summary = LibrarySummary {
            albums: albums.len(),
            artists: artist_list.len(),
            tracks: tracks.len(),
            duration: index.tracks.iter().map(|t| t.duration).sum(),
            bytes: index.tracks.iter().map(|t| t.size).sum(),
            hires,
            lossless,
            scanned_at: (index.scanned_at > 0).then_some(index.scanned_at),
        };

        let hay_albums = album_list.iter().map(|id| norm(&format!("{} {}", albums[id].title, albums[id].artist))).collect();
        let hay_artists = artist_list.iter().map(|id| norm(&artists[id].name)).collect();
        let hay_tracks = track_list
            .iter()
            .map(|id| {
                let t = &views[id];
                norm(&format!("{} {} {}", t.title, t.artist, t.album))
            })
            .collect();

        Library {
            tracks,
            views,
            albums,
            artists,
            genres,
            album_list,
            artist_list,
            genre_list,
            track_list,
            summary,
            hay_albums,
            hay_artists,
            hay_tracks,
        }
    }

    pub fn album_cards(&self) -> Vec<AlbumCard> {
        self.album_list.iter().map(|id| AlbumCard::from(&self.albums[id])).collect()
    }

    pub fn album_tracks(&self, album_id: &str) -> Vec<&Track> {
        self.albums.get(album_id).map(|a| a.track_ids.iter().filter_map(|id| self.views.get(id)).collect()).unwrap_or_default()
    }

    pub fn ordered_tracks(&self) -> impl Iterator<Item = &Track> {
        self.track_list.iter().map(|id| &self.views[id])
    }

    pub fn ordered_artists(&self) -> impl Iterator<Item = &Artist> {
        self.artist_list.iter().map(|id| &self.artists[id])
    }

    pub fn ordered_genres(&self) -> impl Iterator<Item = &Genre> {
        self.genre_list.iter().map(|id| &self.genres[id])
    }

    /// 띄어쓴 낱말이 모두 들어 있으면 맞은 것. 앞에서부터 맞으면 위로 올린다.
    pub fn search(&self, q: &str) -> SearchResult {
        let query = norm(q);
        if query.is_empty() {
            return SearchResult::default();
        }
        let terms: Vec<&str> = query.split(' ').collect();
        let first_term = format!(" {}", terms[0]);
        let rank = |ids: &[String], hay: &[String], limit: usize| -> Vec<String> {
            let mut hits: Vec<(u8, usize)> = Vec::new();
            for (i, h) in hay.iter().enumerate() {
                if !terms.iter().all(|t| h.contains(t)) {
                    continue;
                }
                let score = if h.starts_with(&query) {
                    0
                } else if h.contains(&first_term) {
                    1
                } else {
                    2
                };
                hits.push((score, i));
                if hits.len() > limit * 4 {
                    break;
                }
            }
            hits.sort_by_key(|h| h.0);
            hits.into_iter().take(limit).map(|(_, i)| ids[i].clone()).collect()
        };
        SearchResult {
            artists: rank(&self.artist_list, &self.hay_artists, 12).iter().map(|id| self.artists[id].clone()).collect(),
            albums: rank(&self.album_list, &self.hay_albums, 24).iter().map(|id| AlbumCard::from(&self.albums[id])).collect(),
            tracks: rank(&self.track_list, &self.hay_tracks, 50).iter().map(|id| self.views[id].clone()).collect(),
        }
    }
}

#[derive(Default, Serialize)]
pub struct SearchResult {
    pub artists: Vec<Artist>,
    pub albums: Vec<AlbumCard>,
    pub tracks: Vec<Track>,
}
