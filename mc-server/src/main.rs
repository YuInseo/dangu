use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use mc_server::network::{InstanceSpec, Network};
use mc_server::packets::play::GameMode;
use mc_server::packets::status::ServerInfo;
use mc_server::protocol::{PROTOCOL_VERSION, VERSION_NAME};
use mc_server::server::Config;
use tokio::net::TcpListener;

const AUTOSAVE_INTERVAL: Duration = Duration::from_secs(30);

#[tokio::main]
async fn main() -> std::io::Result<()> {
    // `mc-server master`: 담비 마스터 노드 (프록시 + CLI, 인스턴스마다 자식 프로세스).
    // `mc-server child`: 마스터가 띄우는 인스턴스 (DAMBI_IPC로 마스터에 붙음).
    // 인자 없음: 혼자 도는 서버 (한 프로세스 안의 인스턴스들).
    if std::env::args().nth(1).as_deref() == Some("master") {
        mc_server::dambi::master::run().await?;
        // CLI가 표준 입력을 막고(블로킹 스레드) 있어 런타임이 스스로 끝나지 않으므로
        std::process::exit(0);
    }
    let addr = std::env::var("MC_BIND").unwrap_or_else(|_| "0.0.0.0:25565".into());
    let seed = std::env::var("MC_SEED")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(20260915);
    let world_dir = PathBuf::from(std::env::var("MC_WORLD").unwrap_or_else(|_| "world".into()));

    let info = ServerInfo {
        name: "Dangu MC".into(),
        description: "순수 Rust 커스텀 서버입니다".into(),
        max_players: 100,
    };
    // server.properties의 online-mode, network-compression-threshold에 해당합니다.
    let config = Config {
        online_mode: env_flag("MC_ONLINE_MODE", true),
        compression_threshold: match std::env::var("MC_COMPRESSION")
            .ok()
            .and_then(|v| v.parse::<i64>().ok())
        {
            Some(t) if t < 0 => None,
            Some(t) => Some(t as usize),
            None => Some(256),
        },
        offline_skins: env_flag("MC_OFFLINE_SKINS", false),
        spawn_mobs: env_flag("MC_SPAWN_MOBS", true),
        default_gamemode: std::env::var("MC_GAMEMODE")
            .ok()
            .and_then(|m| GameMode::from_name(&m))
            .unwrap_or(GameMode::Survival),
        // 켜면 모두가 OP 4 (혼자 시험할 때). 끄면 ops.json과 권한 파일을 따릅니다.
        everyone_op: env_flag("MC_EVERYONE_OP", false),
        // 처음 들어오면 OP 4가 되는 이름들 (쉼표로 구분). ops.json에 기록됩니다.
        initial_ops: std::env::var("MC_OPS")
            .map(|v| {
                v.split(',')
                    .map(|s| s.trim().to_owned())
                    .filter(|s| !s.is_empty())
                    .collect()
            })
            .unwrap_or_default(),
        difficulty: std::env::var("MC_DIFFICULTY")
            .ok()
            .and_then(|d| mc_server::server::Difficulty::from_name(&d))
            .unwrap_or(mc_server::server::Difficulty::Normal),
        hardcore: env_flag("MC_HARDCORE", false),
        pregen_radius: std::env::var("MC_PREGEN")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(8),
    };
    // 인스턴스: MC_INSTANCES="lobby,survival:12345:survival,build::creative" (이름[:시드[:모드]]).
    // 처음 켤 때만 쓰이고, 그 뒤로는 world/instances.json이 기준입니다.
    let initial = match std::env::var("MC_INSTANCES") {
        Ok(list) => list
            .split(',')
            .filter(|s| !s.trim().is_empty())
            .map(|item| {
                let mut parts = item.trim().split(':');
                let name = parts.next().unwrap_or("main");
                let mut spec = InstanceSpec::new(name, seed);
                if let Some(s) = parts.next().and_then(|s| s.parse().ok()) {
                    spec.seed = s;
                }
                if let Some(m) = parts.next().and_then(GameMode::from_name) {
                    spec.gamemode = m;
                }
                spec
            })
            .collect(),
        Err(_) => vec![InstanceSpec {
            gamemode: config.default_gamemode,
            difficulty: config.difficulty,
            hardcore: config.hardcore,
            ..InstanceSpec::new("main", seed)
        }],
    };
    let net = Network::open(&world_dir, info, config.clone(), initial)?;
    let server = net.default_instance();
    if config.online_mode {
        server.key(); // 첫 접속이 키 만드는 시간을 기다리지 않도록 미리
        // 서명된 채팅을 확인할 Mojang 키
        match mc_server::auth::fetch_chat_keys().await {
            Ok(keys) => {
                println!(
                    "Mojang 채팅 키 {}개를 받았습니다 (서명된 채팅 확인)",
                    keys.len()
                );
                *server.chat_keys.lock().unwrap() = keys;
            }
            Err(e) => {
                eprintln!("Mojang 채팅 키를 받지 못했습니다 — 채팅은 서명 없이 전합니다: {e}")
            }
        }
    }
    if config.everyone_op {
        println!("MC_EVERYONE_OP: 모든 접속자가 OP 4단계입니다.");
    }
    println!(
        "정품 인증 {}, 압축 {}",
        if config.online_mode {
            "켬"
        } else {
            "끔 (오프라인 모드)"
        },
        config
            .compression_threshold
            .map_or("끔".to_owned(), |t| format!("{t}바이트 이상")),
    );
    for s in net.all() {
        println!(
            "인스턴스 {}{}: 시드 {}, 청크 {}개 올림",
            s.name,
            if s.name == net.default_name() {
                " (기본)"
            } else {
                ""
            },
            s.seed,
            s.world.loaded_chunks()
        );
    }

    let listener = TcpListener::bind(&addr).await?;
    if mc_server::dambi::child::active() {
        let port = listener.local_addr()?.port();
        mc_server::dambi::child::start(Arc::clone(&net), port).await?;
    } else {
        mc_server::dambi::child::init_local(Some(world_dir.join("shared")));
    }
    println!(
        "{addr}에서 듣는 중 — Minecraft {VERSION_NAME} (프로토콜 {PROTOCOL_VERSION}), 시드 {seed}"
    );

    let saver = Arc::clone(&net);
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(AUTOSAVE_INTERVAL);
        tick.tick().await;
        loop {
            tick.tick().await;
            saver.save_all();
        }
    });
    tokio::spawn(console(Arc::clone(&net)));

    tokio::select! {
        result = mc_server::serve_network(listener, Arc::clone(&net)) => result?,
        _ = tokio::signal::ctrl_c() => println!("끄는 중…"),
        _ = server.shutdown.notified() => println!("/stop: 끄는 중…"),
    }
    net.save_all();
    // 세션들이 플레이어를 저장할 틈
    tokio::time::sleep(Duration::from_millis(300)).await;
    Ok(())
}

/// 표준 입력의 줄을 콘솔 명령어로 (`/`는 있어도 없어도 됨). 콘솔은 모든 권한을 가집니다.
async fn console(net: Arc<Network>) {
    use tokio::io::AsyncBufReadExt;
    let mut lines = tokio::io::BufReader::new(tokio::io::stdin()).lines();
    while let Ok(Some(line)) = lines.next_line().await {
        let line = line.trim().trim_start_matches('/');
        if line.is_empty() {
            continue;
        }
        let mut words = line.split_whitespace();
        let name = words.next().unwrap_or_default();
        let args: Vec<&str> = words.collect();
        let actor = mc_server::admin::Actor::console();
        let server = net.default_instance();
        match mc_server::admin::run(&server, &actor, name, &args) {
            Some(out) => out.iter().for_each(|l| println!("{l}")),
            None => println!("콘솔에서 쓸 수 없는 명령어입니다: /{name}"),
        }
    }
}

fn env_flag(name: &str, default: bool) -> bool {
    match std::env::var(name).as_deref() {
        Ok("true" | "1" | "yes" | "on") => true,
        Ok("false" | "0" | "no" | "off") => false,
        _ => default,
    }
}
