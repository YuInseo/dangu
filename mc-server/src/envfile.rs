//! `server.env`: 환경 변수를 파일로 (Spigot의 `server.properties` 자리).
//!
//! 서버를 켤 때 지금 폴더의 `server.env`(`MC_ENV_FILE`로 바꿈)를 읽어 `이름=값` 줄을 환경 변수로
//! 넣습니다. 이미 환경 변수로 준 것이 이깁니다. 파일이 없으면 모든 설정과 설명을 담은 파일을
//! 만들어 둡니다 (처음 켤 때).

use std::path::Path;

/// 처음 켤 때 만드는 파일.
pub const TEMPLATE: &str = r#"# dangu 서버 설정 — 고치고 서버를 다시 켜면 적용됩니다.
# 줄 앞 # 은 설명(꺼진 줄). 환경 변수로 같은 이름을 주면 그것이 이깁니다.

# ---- 기본 ----
# 접속 주소 (포트를 바꾸려면 :뒤 숫자)
MC_BIND=0.0.0.0:25565
# 정품 인증 (false = 오프라인 모드, 같은 컴퓨터에서 시험할 때)
MC_ONLINE_MODE=true
# 처음 들어오면 OP 4가 될 마인크래프트 이름 (쉼표로 여럿)
MC_OPS=
# 처음 들어온 사람의 게임 모드: survival creative adventure spectator
MC_GAMEMODE=survival
# peaceful easy normal hard
MC_DIFFICULTY=normal
MC_HARDCORE=false
# 몹 자연 스폰
MC_SPAWN_MOBS=true

# ---- 월드 ----
# 모든 데이터가 들어가는 폴더
MC_WORLD=world
# 처음 만들 때의 시드
MC_SEED=20260915
# 처음 켤 때 만들 인스턴스: 이름[:시드[:모드]] 를 쉼표로 (예: square,town,wild:123)
MC_INSTANCES=main
# 새 월드 스폰 둘레를 미리 만들 반지름 (청크, 0 = 안 함)
MC_PREGEN=8

# ---- 리소스 팩 ----
# 팩을 내려 주는 포트 (0 = 끔)
MC_PACK_PORT=25566
# 밖에서 보이는 팩 주소 (비우면 접속한 주소로 저절로)
#MC_PACK_URL=http://example.com:25566/pack.zip
# 팩을 거절하면 내보내기
MC_PACK_REQUIRED=true
# HUD(재화·퀘스트·스탯 판)
MC_HUD=true

# ---- 웹 관리 화면 ----
# 0 = 끔. 켜면 콘솔에 http://127.0.0.1:25580/#token=… 주소가 찍힘
MC_ADMIN_PORT=25580
# 밖에서 열려면 0.0.0.0 (토큰만으로 지키니 SSH 터널을 권함)
MC_ADMIN_BIND=127.0.0.1
# 비우면 world/admin_token.txt 에 만들어 둠
#MC_ADMIN_TOKEN=

# ---- 그 밖 ----
# 압축 기준 (바이트, 음수 = 끔)
MC_COMPRESSION=256
# 오프라인 모드에서 같은 이름의 정품 스킨 빌리기
MC_OFFLINE_SKINS=false
# 혼자 시험할 때 모두 OP
MC_EVERYONE_OP=false
"#;

/// `이름=값` 줄들 (따옴표는 벗김, 빈 값도 넣음).
pub fn parse(text: &str) -> Vec<(String, String)> {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .filter_map(|l| {
            let l = l.strip_prefix("export ").unwrap_or(l);
            let (k, v) = l.split_once('=')?;
            let v = v.trim();
            let v = v
                .strip_prefix('"')
                .and_then(|v| v.strip_suffix('"'))
                .or_else(|| v.strip_prefix('\'').and_then(|v| v.strip_suffix('\'')))
                .unwrap_or(v);
            let k = k.trim();
            (!k.is_empty()).then(|| (k.to_owned(), v.to_owned()))
        })
        .collect()
}

/// 파일을 읽어 아직 없는 환경 변수를 넣습니다. 처음이면 파일을 만듭니다.
/// 스레드를 띄우기 전에 불러야 합니다 (`set_var`).
pub fn load() {
    let path = std::env::var_os("MC_ENV_FILE").unwrap_or_else(|| "server.env".into());
    let path = Path::new(&path);
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(_) => {
            if std::fs::write(path, TEMPLATE).is_ok() {
                println!(
                    "처음 켜는 것 같아 설정 파일 {}을(를) 만들었습니다. 고친 뒤 다시 켜면 적용됩니다.",
                    path.display()
                );
            }
            TEMPLATE.to_owned()
        }
    };
    for (k, v) in parse(&text) {
        if v.is_empty() || std::env::var_os(&k).is_some() {
            continue;
        }
        // SAFETY: main에서 런타임·스레드를 만들기 전에 한 번 부릅니다.
        unsafe { std::env::set_var(&k, &v) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_lines_quotes_and_comments() {
        let v = parse("# 설명\nA=1\n export B = \"x y\" \nC='z'\n\nD=\nbad\n");
        assert_eq!(
            v,
            vec![
                ("A".into(), "1".into()),
                ("B".into(), "x y".into()),
                ("C".into(), "z".into()),
                ("D".into(), String::new()),
            ]
        );
        // 처음 만드는 파일도 그대로 읽힘
        assert!(
            parse(TEMPLATE)
                .iter()
                .any(|(k, v)| k == "MC_BIND" && v == "0.0.0.0:25565")
        );
    }
}
