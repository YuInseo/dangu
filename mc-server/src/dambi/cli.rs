//! 마스터 관리자 CLI: 표준 입력 한 줄 = 명령어 하나. 영어와 한국어 이름 둘 다 받습니다.
//!
//! ```text
//! instance start rpg --ram 4G        인스턴스 시작 rpg --램 4G
//! instance stop rpg                  인스턴스 중지 rpg
//! instance list                      인스턴스 목록
//! move Steve rpg                     이동 Steve rpg
//! money Steve give 1000 cash         재화 Steve 지급 1000 캐시
//! ```

use std::sync::Arc;

use tokio::io::AsyncBufReadExt;

use super::ipc::Msg;
use super::master::Master;
use super::process::{InstanceConfig, State, format_bytes, parse_bytes};

pub async fn run(master: Arc<Master>) {
    let mut lines = tokio::io::BufReader::new(tokio::io::stdin()).lines();
    while let Ok(Some(line)) = lines.next_line().await {
        for out in execute(&master, &line) {
            println!("{out}");
        }
    }
}

const HELP: &[&str] = &[
    "instance start <이름> [--ram 4G] [--port N] [--seed N] [--mode survival]  (인스턴스 시작)",
    "instance stop|restart <이름>                                              (인스턴스 중지|재시작)",
    "instance list                                                             (인스턴스 목록)",
    "instance default <이름>   처음 접속하는 사람이 갈 곳                       (인스턴스 기본)",
    "instance ram <이름> <크기|off>  다음 시작부터 적용                         (인스턴스 램)",
    "send <인스턴스> <명령어…>   그 인스턴스 콘솔에서 실행                       (전달)",
    "broadcast <내용>          모든 인스턴스에 공지                              (공지)",
    "players                   접속자와 있는 곳                                  (접속자)",
    "move <플레이어> <인스턴스>                                                  (이동)",
    "money <플레이어> [give|take|set <양> [money|cash|afk]]                      (재화 … 지급|회수|설정)",
    "iazip                     contents를 다시 읽어 리소스 팩을 새로 만들고 모두에게 (팩)",
    "stop                      모든 인스턴스를 저장하고 끝냄                      (종료)",
];

fn alias(word: &str) -> &str {
    match word {
        "인스턴스" => "instance",
        "시작" => "start",
        "중지" | "정지" => "stop",
        "재시작" => "restart",
        "목록" => "list",
        "기본" => "default",
        "램" => "ram",
        "전달" => "send",
        "공지" => "broadcast",
        "접속자" => "players",
        "이동" => "move",
        "재화" | "돈" => "money",
        "지급" => "give",
        "회수" => "take",
        "설정" => "set",
        "종료" => "stop",
        "도움말" | "?" => "help",
        "--램" => "--ram",
        "--포트" => "--port",
        "--시드" => "--seed",
        "--모드" => "--mode",
        w => w,
    }
}

/// 한 줄을 실행하고 보여 줄 줄들을 돌려줍니다 (시험하기 쉽게 출력과 나눔).
pub fn execute(master: &Arc<Master>, line: &str) -> Vec<String> {
    let line = line.trim().trim_start_matches('/');
    if line.is_empty() {
        return Vec::new();
    }
    let words: Vec<&str> = line.split_whitespace().map(alias).collect();
    let raw: Vec<&str> = line.split_whitespace().collect();
    let err = |s: String| vec![format!("✗ {s}")];
    match words.as_slice() {
        ["help"] => HELP.iter().map(|s| s.to_string()).collect(),
        ["instance", "list"] | ["instance"] => list(master),
        ["instance", "start", name, opts @ ..] => {
            let mut c = master
                .config(name)
                .unwrap_or_else(|| InstanceConfig::new(name, master.free_port()));
            if let Err(e) = apply_options(&mut c, opts) {
                return err(e);
            }
            if let Err(e) = master.upsert_config(c.clone()) {
                return err(e.to_string());
            }
            match master.start(name) {
                Ok(pid) => vec![format!(
                    "✓ {name} 켬 (pid {pid}, 127.0.0.1:{}, 램 {})",
                    c.port,
                    c.ram.map_or("제한 없음".into(), format_bytes)
                )],
                Err(e) => err(e),
            }
        }
        ["instance", "stop", name] => match master.stop(name) {
            Ok(()) => vec![format!("✓ {name}: 저장하고 끄는 중")],
            Err(e) => err(e),
        },
        ["instance", "restart", name] => {
            let name = name.to_string();
            if let Err(e) = master.stop(&name) {
                return err(e);
            }
            let m = Arc::clone(master);
            let reply = vec![format!("✓ {name}: 재시작 중")];
            tokio::spawn(async move {
                for _ in 0..100 {
                    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
                    if !m.running_names().contains(&name) {
                        match m.start(&name) {
                            Ok(pid) => println!("✓ {name} 다시 켬 (pid {pid})"),
                            Err(e) => println!("✗ {e}"),
                        }
                        return;
                    }
                }
                println!("✗ {name}이(가) 꺼지지 않아 다시 켜지 못했습니다");
            });
            reply
        }
        ["instance", "default", name] => match master.set_default(name) {
            Ok(()) => vec![format!("✓ 처음 접속하면 {name}(으)로 갑니다")],
            Err(e) => err(e),
        },
        ["instance", "ram", name, size] => {
            let Some(mut c) = master.config(name) else {
                return err(format!("{name}: 설정이 없습니다"));
            };
            c.ram = if *size == "off" || *size == "끔" {
                None
            } else {
                match parse_bytes(size) {
                    Some(b) => Some(b),
                    None => return err(format!("크기를 모르겠습니다: {size} (예: 4G, 512M)")),
                }
            };
            match master.upsert_config(c) {
                Ok(()) => vec![format!("✓ {name}: 다음 시작부터 램 {size}")],
                Err(e) => err(e.to_string()),
            }
        }
        ["send", instance, ..] if raw.len() > 2 => {
            let command = raw[2..].join(" ");
            if master.send(
                instance,
                Msg::Command {
                    line: command.clone(),
                },
            ) {
                vec![format!("→ {instance}: /{command}")]
            } else {
                err(format!("{instance}은(는) 연결되어 있지 않습니다"))
            }
        }
        ["broadcast", ..] if raw.len() > 1 => {
            let text = raw[1..].join(" ");
            master.broadcast(Msg::Event {
                channel: "broadcast".into(),
                payload: text.clone(),
            });
            vec![format!("✓ 공지: {text}")]
        }
        ["players"] => {
            let p = master.players();
            if p.is_empty() {
                return vec!["접속자 없음".into()];
            }
            let mut out = vec![format!("접속자 {}명", p.len())];
            out.extend(
                p.iter()
                    .map(|(_, o)| format!("  {} — {}", o.name, o.instance)),
            );
            out
        }
        ["move", player, to] => {
            let Some((uuid, o)) = master.find_player(player) else {
                return err(format!("{player}님은 접속해 있지 않습니다"));
            };
            match master.transfer(uuid, &o.name, to) {
                Ok(()) => vec![format!("✓ {} → {to}", o.name)],
                Err(e) => err(e),
            }
        }
        ["money", player, rest @ ..] => money(master, player, rest),
        ["iazip"] | ["iareload"] | ["팩"] => {
            let l = crate::plugins::contents::reload_and_rebuild();
            master.broadcast(Msg::Event {
                channel: crate::plugins::contents::PACK_RELOAD.into(),
                payload: String::new(),
            });
            crate::plugins::contents::summary(&l)
        }
        ["stop"] => {
            master.shutdown.notify_one();
            vec!["모든 인스턴스를 저장하고 끕니다…".into()]
        }
        _ => vec![format!("모르는 명령어: {line} (help 참고)")],
    }
}

fn apply_options(c: &mut InstanceConfig, opts: &[&str]) -> Result<(), String> {
    let mut it = opts.iter();
    while let Some(&flag) = it.next() {
        let value = it
            .next()
            .ok_or_else(|| format!("{flag} 뒤에 값이 없습니다"))?;
        match flag {
            "--ram" => {
                c.ram = Some(parse_bytes(value).ok_or(format!("크기를 모르겠습니다: {value}"))?)
            }
            "--port" => {
                c.port = value
                    .parse()
                    .map_err(|_| format!("포트가 아닙니다: {value}"))?
            }
            "--seed" => {
                c.seed = Some(
                    value
                        .parse()
                        .map_err(|_| format!("시드가 아닙니다: {value}"))?,
                )
            }
            "--mode" => c.gamemode = Some(value.to_string()),
            _ => return Err(format!("모르는 옵션 {flag}")),
        }
    }
    Ok(())
}

fn list(master: &Master) -> Vec<String> {
    let running = master.status();
    let default = master.default_name();
    master
        .configs()
        .iter()
        .map(|c| {
            let mark = if c.name == default { "*" } else { " " };
            match running.iter().find(|r| r.0 == c.name) {
                Some((_, pid, port, state, online, rss, ram, cgroup, up)) => format!(
                    "{mark} {:<8} {} pid {pid:<7} :{port} 접속 {online:<3} 램 {}/{}{} 켠 지 {}초",
                    c.name,
                    match state {
                        State::Starting => "시작 중",
                        State::Ready => "켜짐  ",
                        State::Stopping => "끄는 중",
                    },
                    format_bytes(*rss),
                    ram.map_or("∞".into(), format_bytes),
                    if *cgroup { " (cgroup)" } else { "" },
                    up.as_secs()
                ),
                None => format!(
                    "{mark} {:<8} 꺼짐    :{} 램 {}",
                    c.name,
                    c.port,
                    c.ram.map_or("∞".into(), format_bytes)
                ),
            }
        })
        .collect()
}

fn money(master: &Master, player: &str, rest: &[&str]) -> Vec<String> {
    let Some((uuid, o)) = master.find_player(player) else {
        return vec![format!("✗ {player}님은 접속해 있지 않습니다")];
    };
    let show = |d: &super::shared::SharedData| {
        vec![format!(
            "{}: 돈 {} · 캐시 {} · 잠수포인트 {} · 플레이 {}분",
            o.name,
            d.money,
            d.cash,
            d.afk,
            d.playtime_secs / 60
        )]
    };
    let (op, amount, field) = match rest {
        [] => return show(&master.shared.get(uuid)),
        [op, amount] => (*op, *amount, "money"),
        [op, amount, field] => (*op, *amount, alias_field(field)),
        _ => return vec!["✗ money <플레이어> [give|take|set <양> [money|cash|afk]]".into()],
    };
    let Ok(n) = amount.parse::<i64>() else {
        return vec![format!("✗ 숫자가 아닙니다: {amount}")];
    };
    let (ok, data) = master.shared.modify(uuid, |d| match op {
        "give" => d.add(field, n),
        "take" => d.add(field, -n),
        "set" => match d.field_mut(field) {
            Some(v) if n >= 0 => {
                *v = n;
                true
            }
            _ => false,
        },
        _ => false,
    });
    if ok {
        master.send(
            &o.instance,
            Msg::SharedChanged {
                uuid,
                data: data.clone(),
            },
        );
        show(&data)
    } else {
        vec![format!(
            "✗ 바꾸지 못했습니다 (잔액 부족이거나 모르는 재화 '{field}')"
        )]
    }
}

fn alias_field(f: &str) -> &str {
    match f {
        "돈" => "money",
        "캐시" => "cash",
        "잠수" | "잠수포인트" => "afk",
        f => f,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::packets::status::ServerInfo;

    #[tokio::test]
    async fn korean_and_english_commands_manage_configs() {
        let dir = std::env::temp_dir().join(format!("dambi-cli-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let m = Master::new(
            &dir,
            ServerInfo {
                name: "t".into(),
                description: "t".into(),
                max_players: 1,
            },
        )
        .unwrap();
        // 처음 켜면 마을·야생·rpg
        assert_eq!(m.configs().len(), 3);
        let out = execute(&m, "인스턴스 램 rpg 4G");
        assert!(out[0].starts_with('✓'), "{out:?}");
        assert_eq!(m.config("rpg").unwrap().ram, Some(4 << 30));
        let out = execute(&m, "instance default 야생");
        assert!(out[0].starts_with('✓'));
        assert_eq!(m.default_name(), "야생");
        assert!(execute(&m, "인스턴스 목록").len() >= 3);
        assert!(
            execute(&m, "재화 Nobody")
                .iter()
                .any(|l| l.contains("접속해 있지"))
        );
        assert!(execute(&m, "instance stop rpg")[0].contains("꺼져"));
        // 다음에 켜도 남아 있음
        let again = Master::new(&dir, m.info.clone()).unwrap();
        assert_eq!(again.config("rpg").unwrap().ram, Some(4 << 30));
        std::fs::remove_dir_all(dir).ok();
    }
}
