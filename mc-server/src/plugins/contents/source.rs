//! 콘텐츠 팩을 모읍니다: 디스크의 `contents/<팩>/` (ItemsAdder의 다섯 가지 폴더 구조와
//! ItemsAdder 3의 `items_packs`/`resource_pack`), 그리고 서버에 들어 있는 팩.
//!
//! 어느 구조든 결과는 같습니다: 설정 파일들과, 리소스 팩 안 경로(`assets/<ns>/...`)로 옮긴
//! 파일들.

use std::path::Path;

/// 팩 하나.
pub struct Pack {
    /// 폴더 이름 (구조 1에서는 이름공간).
    pub name: String,
    /// (보여 줄 위치, 설정 내용).
    pub configs: Vec<(String, String)>,
    /// (리소스 팩 안 경로, 내용).
    pub files: Vec<(String, Vec<u8>)>,
}

/// 구조 1(`contents/<ns>/textures/...`)에서 바로 `assets/<ns>/` 아래로 가는 폴더들.
const ASSET_DIRS: &[&str] = &[
    "textures",
    "models",
    "sounds",
    "font",
    "lang",
    "blockstates",
    "items",
    "particles",
    "shaders",
    "texts",
    "atlases",
    "equipment",
    "post_effect",
    "waypoint_style",
    "optifine",
];
const ASSET_FILES: &[&str] = &["sounds.json"];

fn walk(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<_> = rd.flatten().map(|e| e.path()).collect();
    entries.sort();
    for p in entries {
        if p.file_name()
            .is_some_and(|n| n.to_string_lossy().starts_with('.'))
        {
            continue;
        }
        if p.is_dir() {
            walk(&p, out);
        } else {
            out.push(p);
        }
    }
}

fn parts(base: &Path, p: &Path) -> Vec<String> {
    p.strip_prefix(base)
        .unwrap_or(p)
        .components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect()
}

fn is_config(name: &str) -> bool {
    name.ends_with(".yml") || name.ends_with(".yaml")
}

/// 팩 폴더 안의 한 파일이 리소스 팩의 어디로 가는지 (구조 1~5). 설정·무관한 파일은 None.
pub fn asset_path(pack: &str, rel: &[String]) -> Option<String> {
    let first = rel.first()?.as_str();
    let rest = |from: usize| rel[from..].join("/");
    match first {
        "configs" => None,
        // 구조 2: resourcepack/assets/<ns>/..., 구조 3: resourcepack/<ns>/...
        "resourcepack" | "resource_pack" => match rel.get(1)?.as_str() {
            "assets" if rel.len() > 3 => Some(format!("assets/{}", rest(2))),
            "assets" => None,
            _ if rel.len() > 2 => Some(format!("assets/{}", rest(1))),
            _ => None,
        },
        // 구조 4: assets/<ns>/...
        "assets" if rel.len() > 2 => Some(rest(0)),
        // 구조 1: textures/..., models/... → assets/<팩 이름>/...
        d if ASSET_DIRS.contains(&d) && rel.len() > 1 => Some(format!("assets/{pack}/{}", rest(0))),
        f if ASSET_FILES.contains(&f) && rel.len() == 1 => Some(format!("assets/{pack}/{f}")),
        // 구조 5: <ns>/textures/... (이름공간 폴더가 바로)
        ns if rel.len() > 2 && ASSET_DIRS.contains(&rel[1].as_str()) => {
            Some(format!("assets/{ns}/{}", rest(1)))
        }
        _ => None,
    }
}

/// `base`(contents 폴더) 아래의 모든 팩. ItemsAdder 3의 `data/` 폴더도 알아봅니다.
pub fn from_disk(base: &Path) -> Vec<Pack> {
    let mut packs = Vec::new();
    // ItemsAdder 3: data/items_packs/<ns>/*.yml + data/resource_pack/assets/...
    let legacy_items = base.join("items_packs");
    let legacy_rp = base.join("resource_pack");
    if legacy_items.is_dir() || legacy_rp.is_dir() {
        let mut configs = Vec::new();
        let mut paths = Vec::new();
        walk(&legacy_items, &mut paths);
        for p in paths
            .into_iter()
            .filter(|p| is_config(&p.to_string_lossy()))
        {
            if let Ok(t) = std::fs::read_to_string(&p) {
                configs.push((parts(base, &p).join("/"), t));
            }
        }
        let mut files = Vec::new();
        let mut paths = Vec::new();
        walk(&legacy_rp, &mut paths);
        for p in paths {
            let rel = parts(&legacy_rp, &p);
            if rel.first().is_some_and(|f| f == "assets")
                && let Ok(b) = std::fs::read(&p)
            {
                files.push((rel.join("/"), b));
            }
        }
        packs.push(Pack {
            name: "items_packs".into(),
            configs,
            files,
        });
    }
    let Ok(rd) = std::fs::read_dir(base) else {
        return packs;
    };
    let mut dirs: Vec<_> = rd
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    dirs.sort();
    for dir in dirs {
        let name = dir.file_name().unwrap().to_string_lossy().into_owned();
        if name.starts_with('.') || name == "items_packs" || name == "resource_pack" {
            continue;
        }
        let mut paths = Vec::new();
        walk(&dir, &mut paths);
        let mut pack = Pack {
            name: name.clone(),
            configs: Vec::new(),
            files: Vec::new(),
        };
        for p in paths {
            let rel = parts(&dir, &p);
            let file = rel.last().cloned().unwrap_or_default();
            if is_config(&file)
                && rel
                    .first()
                    .is_some_and(|f| f == "configs" || rel.len() == 1)
            {
                if let Ok(t) = std::fs::read_to_string(&p) {
                    pack.configs.push((parts(base, &p).join("/"), t));
                }
            } else if let Some(to) = asset_path(&name, &rel)
                && let Ok(b) = std::fs::read(&p)
            {
                pack.files.push((to, b));
            }
        }
        packs.push(pack);
    }
    packs
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(s: &str) -> Vec<String> {
        s.split('/').map(String::from).collect()
    }

    #[test]
    fn all_five_folder_structures() {
        let want = Some("assets/myitems/textures/item/a.png".to_owned());
        assert_eq!(asset_path("myitems", &p("textures/item/a.png")), want, "1");
        assert_eq!(
            asset_path("x", &p("resourcepack/assets/myitems/textures/item/a.png")),
            want,
            "2"
        );
        assert_eq!(
            asset_path("x", &p("resourcepack/myitems/textures/item/a.png")),
            want,
            "3"
        );
        assert_eq!(
            asset_path("x", &p("assets/myitems/textures/item/a.png")),
            want,
            "4"
        );
        assert_eq!(
            asset_path("x", &p("myitems/textures/item/a.png")),
            want,
            "5"
        );
        assert_eq!(asset_path("x", &p("configs/a.yml")), None);
        assert_eq!(
            asset_path("x", &p("resourcepack/assets/minecraft/font/default.json")),
            Some("assets/minecraft/font/default.json".into())
        );
        assert_eq!(
            asset_path("snd", &p("sounds.json")),
            Some("assets/snd/sounds.json".into())
        );
    }
}
