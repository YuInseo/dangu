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
    let master = std::env::args().nth(1).as_deref() == Some("master");
    // ItemsAdder식 콘텐츠 (MC_CONTENTS, 기본 ./contents): 아이템·블록·이모지 + 리소스 팩
    // HUD 그림·셰이더를 팩에 넣을지 (콘텐츠를 읽기 전에 정해야 함)
    mc_server::plugins::hud::set_enabled(env_flag("MC_HUD", true));
    // 길드: 모든 인스턴스(담비 자식 프로세스 포함)가 같은 파일
    mc_server::plugins::guild::config::open_global();
    mc_server::plugins::quest::open_global();
    // 장비 내용(아이템 목록이 여기서 나오므로 콘텐츠보다 먼저)과 장비 하나하나의 상태
    mc_server::plugins::equip::open_global();
    // 금지 아이템 (제작·드랍 막기)
    mc_server::plugins::rules::open_global();
    mc_server::plugins::rank::open_global();
    mc_server::plugins::guild::store::open_global(
        std::env::var_os("MC_GUILDS")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| "world/guilds.json".into()),
    );
    let loaded = mc_server::plugins::contents::reload();
    if !loaded.items.is_empty() || !loaded.errors.is_empty() {
        for l in mc_server::plugins::contents::summary(&loaded) {
            println!("{l}");
        }
    }
    // 우리 아이템 그림(리소스 팩): MC_PACK_PORT(기본 25566, 0이면 끔)에서 내려 줍니다. 자식
    // 인스턴스는 마스터가 내려 주는 팩을 알려 주기만 합니다.
    let pack_port: u16 = std::env::var("MC_PACK_PORT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(25566);
    if pack_port != 0 {
        mc_server::resource_pack::enable(
            pack_port,
            std::env::var("MC_PACK_URL").ok(),
            env_flag("MC_PACK_REQUIRED", true),
        );
        if !mc_server::dambi::child::active() {
            match TcpListener::bind(("0.0.0.0", pack_port)).await {
                Ok(l) => {
                    println!("리소스 팩을 :{pack_port}에서 내려 줍니다");
                    tokio::spawn(mc_server::resource_pack::serve(l));
                }
                Err(e) => eprintln!("리소스 팩 포트 {pack_port}를 열지 못했습니다: {e}"),
            }
        }
    }
    // 웹 관리 화면 (MC_ADMIN_PORT, 기본 127.0.0.1:25580). 담비 자식은 마스터 것을 같이 씀
    if !mc_server::dambi::child::active()
        && let Some((addr, token)) = mc_server::plugins::webadmin::settings()
    {
        match TcpListener::bind(&addr).await {
            Ok(l) => {
                println!("웹 관리 화면: http://{addr}/#token={token}");
                tokio::spawn(mc_server::plugins::webadmin::serve(l, token));
            }
            Err(e) => eprintln!("웹 관리 화면 {addr}을 열지 못했습니다: {e}"),
        }
    }
    if master {
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
        lottery_npc: env_flag("MC_LOTTERY_NPC", true),
        chat_channels: env_flag("MC_CHAT_CHANNELS", true),
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
