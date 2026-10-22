//! 인스턴스 = 독립된 OS 자식 프로세스 하나 (안에 오버월드·네더·엔드 세 차원).
//!
//! 마스터는 자기 실행 파일을 `child` 모드로 다시 띄웁니다. 램 할당량은 두 겹으로 지킵니다.
//! 1. cgroup v2를 쓸 수 있으면 `<cgroup>/dambi/<이름>/memory.max`에 적어 커널이 막게 합니다.
//! 2. 언제나 `/proc/<pid>/status`의 VmRSS를 2초마다 보고, 90%를 넘으면 경고, 할당량을 3번
//!    연달아 넘으면 저장하고 끄라고(`Shutdown`) 한 뒤 10초 안에 안 꺼지면 죽입니다.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::{Duration, Instant};

use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};

use crate::json::{self, Json};

/// 마스터가 기억하는 인스턴스 설정 (`<기준>/dambi.json`).
#[derive(Debug, Clone, PartialEq)]
pub struct InstanceConfig {
    pub name: String,
    /// 자식이 듣는 내부 포트 (127.0.0.1에만).
    pub port: u16,
    /// 램 할당량 (바이트). 없으면 제한 없음.
    pub ram: Option<u64>,
    pub seed: Option<i64>,
    pub gamemode: Option<String>,
    /// 마스터가 켤 때 같이 켬.
    pub autostart: bool,
}

impl InstanceConfig {
    pub fn new(name: &str, port: u16) -> Self {
        Self {
            name: name.to_owned(),
            port,
            ram: None,
            seed: None,
            gamemode: None,
            autostart: true,
        }
    }

    pub fn to_json(&self) -> Json {
        let mut f = vec![
            ("name".to_owned(), Json::String(self.name.clone())),
            ("port".to_owned(), Json::Number(self.port as f64)),
            ("autostart".to_owned(), Json::Bool(self.autostart)),
        ];
        if let Some(r) = self.ram {
            f.push(("ram".into(), Json::String(format_bytes(r))));
        }
        if let Some(s) = self.seed {
            f.push(("seed".into(), Json::Number(s as f64)));
        }
        if let Some(g) = &self.gamemode {
            f.push(("gamemode".into(), Json::String(g.clone())));
        }
        Json::Object(f)
    }

    pub fn from_json(j: &Json) -> Option<Self> {
        Some(Self {
            name: j.get("name")?.as_str()?.to_owned(),
            port: j.get("port")?.as_f64()? as u16,
            ram: j.get("ram").and_then(Json::as_str).and_then(parse_bytes),
            seed: j.get("seed").and_then(Json::as_f64).map(|s| s as i64),
            gamemode: j.get("gamemode").and_then(Json::as_str).map(str::to_owned),
            autostart: j.get("autostart").and_then(Json::as_bool).unwrap_or(true),
        })
    }
}

pub fn load_configs(path: &Path) -> Vec<InstanceConfig> {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|t| json::parse(&t).ok())
        .and_then(|j| {
            j.get("instances")?
                .as_array()
                .map(|a| a.iter().filter_map(InstanceConfig::from_json).collect())
        })
        .unwrap_or_default()
}

pub fn save_configs(path: &Path, configs: &[InstanceConfig]) -> std::io::Result<()> {
    let j = Json::Object(vec![(
        "instances".into(),
        Json::Array(configs.iter().map(InstanceConfig::to_json).collect()),
    )]);
    std::fs::write(path, j.pretty())
}

/// `4G`, `512M`, `1.5g`, `800000k`, `1073741824` → 바이트.
pub fn parse_bytes(s: &str) -> Option<u64> {
    let s = s.trim();
    let (num, unit) = match s.char_indices().find(|(_, c)| c.is_ascii_alphabetic()) {
        Some((i, _)) => (&s[..i], s[i..].to_ascii_lowercase()),
        None => (s, String::new()),
    };
    let n: f64 = num.trim().parse().ok()?;
    let mul: u64 = match unit.trim_end_matches(['b', 'i']) {
        "" => 1,
        "k" => 1 << 10,
        "m" => 1 << 20,
        "g" => 1 << 30,
        "t" => 1 << 40,
        _ => return None,
    };
    (n > 0.0).then_some((n * mul as f64) as u64)
}

pub fn format_bytes(b: u64) -> String {
    if b >= 1 << 30 && b.is_multiple_of(1 << 30) {
        format!("{}G", b >> 30)
    } else if b >= 1 << 30 {
        format!("{:.1}G", b as f64 / (1u64 << 30) as f64)
    } else {
        format!("{}M", b >> 20)
    }
}

/// 프로세스의 실제 메모리 (RSS, 바이트). 리눅스가 아니면 None.
pub fn rss_of(pid: u32) -> Option<u64> {
    let status = std::fs::read_to_string(format!("/proc/{pid}/status")).ok()?;
    let line = status.lines().find(|l| l.starts_with("VmRSS:"))?;
    let kib: u64 = line.split_whitespace().nth(1)?.parse().ok()?;
    Some(kib * 1024)
}

/// cgroup v2에 이 프로세스를 넣고 메모리 상한을 겁니다. 안 되면 이유를 돌려줍니다.
fn cgroup_limit(name: &str, pid: u32, bytes: u64) -> Result<PathBuf, String> {
    let root = Path::new("/sys/fs/cgroup");
    let controllers = std::fs::read_to_string(root.join("cgroup.controllers"))
        .map_err(|_| "cgroup v2가 없음".to_owned())?;
    if !controllers.split_whitespace().any(|c| c == "memory") {
        return Err("memory 컨트롤러가 없음".into());
    }
    let parent = root.join("dambi");
    std::fs::create_dir_all(&parent).map_err(|e| format!("{}: {e}", parent.display()))?;
    // 부모에서 memory를 자식에게 넘겨야 memory.max가 생깁니다 (이미 켜져 있으면 무시).
    let _ = std::fs::write(root.join("cgroup.subtree_control"), "+memory");
    let _ = std::fs::write(parent.join("cgroup.subtree_control"), "+memory");
    let dir = parent.join(name);
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    std::fs::write(dir.join("memory.max"), bytes.to_string())
        .map_err(|e| format!("memory.max: {e}"))?;
    // 스왑으로 새지 않게 (없는 커널도 있음)
    let _ = std::fs::write(dir.join("memory.swap.max"), "0");
    std::fs::write(dir.join("cgroup.procs"), pid.to_string())
        .map_err(|e| format!("cgroup.procs: {e}"))?;
    Ok(dir)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    /// 띄웠고 `Hello`를 기다리는 중.
    Starting,
    /// 접속을 받음.
    Ready,
    /// 끄라고 했음.
    Stopping,
}

pub struct Running {
    pub config: InstanceConfig,
    pub pid: u32,
    pub started: Instant,
    pub state: State,
    /// 커널이 지키는 할당량이면 그 cgroup.
    pub cgroup: Option<PathBuf>,
    pub online: u32,
    pub rss: u64,
    over_limit_checks: u32,
    warned: bool,
    stop_deadline: Option<Instant>,
    child: Child,
}

impl Running {
    /// 띄웁니다. `base`는 마스터 기준 폴더 (월드는 `<base>/instances/<이름>`).
    pub fn spawn(config: &InstanceConfig, base: &Path, ipc: &Path) -> std::io::Result<Running> {
        let exe = std::env::current_exe()?;
        let world = base.join("instances").join(&config.name);
        std::fs::create_dir_all(&world)?;
        let mut cmd = Command::new(exe);
        cmd.arg("child")
            .env("DAMBI_NAME", &config.name)
            .env("DAMBI_IPC", ipc)
            .env("MC_BIND", format!("127.0.0.1:{}", config.port))
            .env("MC_WORLD", &world)
            .env_remove("MC_INSTANCES")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        if let Some(seed) = config.seed {
            cmd.env("MC_SEED", seed.to_string());
        }
        if let Some(g) = &config.gamemode {
            cmd.env("MC_GAMEMODE", g);
        }
        if let Some(r) = config.ram {
            // 자식이 청크 캐시 크기를 고를 때 참고합니다.
            cmd.env("DAMBI_RAM", r.to_string());
        }
        let mut child = cmd.spawn()?;
        let pid = child.id().unwrap_or(0);
        // 자식의 출력에 이름표를 붙여 마스터 터미널로
        for (pipe, err) in [
            (
                child
                    .stdout
                    .take()
                    .map(|p| Box::new(p) as Box<dyn tokio::io::AsyncRead + Unpin + Send>),
                false,
            ),
            (
                child
                    .stderr
                    .take()
                    .map(|p| Box::new(p) as Box<dyn tokio::io::AsyncRead + Unpin + Send>),
                true,
            ),
        ] {
            let Some(pipe) = pipe else { continue };
            let tag = config.name.clone();
            tokio::spawn(async move {
                let mut lines = BufReader::new(pipe).lines();
                while let Ok(Some(l)) = lines.next_line().await {
                    if err {
                        eprintln!("[{tag}] {l}");
                    } else {
                        println!("[{tag}] {l}");
                    }
                }
            });
        }
        let cgroup = match config.ram {
            Some(bytes) => match cgroup_limit(&config.name, pid, bytes) {
                Ok(dir) => Some(dir),
                Err(why) => {
                    println!(
                        "[마스터] {}: cgroup으로 램을 묶지 못해 ({why}) RSS 감시로 지킵니다",
                        config.name
                    );
                    None
                }
            },
            None => None,
        };
        Ok(Running {
            config: config.clone(),
            pid,
            started: Instant::now(),
            state: State::Starting,
            cgroup,
            online: 0,
            rss: 0,
            over_limit_checks: 0,
            warned: false,
            stop_deadline: None,
            child,
        })
    }

    /// 2초마다: 끝났는지, 램을 넘었는지. 돌려주는 값 = 할 일.
    pub fn watch(&mut self) -> Watch {
        if let Ok(Some(status)) = self.child.try_wait() {
            return Watch::Exited(status.code());
        }
        if let Some(rss) = rss_of(self.pid) {
            self.rss = rss;
        }
        if let Some(deadline) = self.stop_deadline {
            if Instant::now() > deadline {
                let _ = self.child.start_kill();
                return Watch::Killed;
            }
            return Watch::Fine;
        }
        let Some(limit) = self.config.ram else {
            return Watch::Fine;
        };
        if self.rss > limit {
            self.over_limit_checks += 1;
            if self.over_limit_checks >= 3 {
                return Watch::OverLimit;
            }
        } else {
            self.over_limit_checks = 0;
        }
        if self.rss > limit / 10 * 9 && !self.warned {
            self.warned = true;
            return Watch::Warn;
        }
        if self.rss < limit / 10 * 8 {
            self.warned = false;
        }
        Watch::Fine
    }

    /// 끄라고 한 뒤 기다릴 시간. 넘기면 [`Running::watch`]가 죽입니다.
    pub fn begin_stop(&mut self, grace: Duration) {
        self.state = State::Stopping;
        self.stop_deadline = Some(Instant::now() + grace);
    }

    pub fn kill(&mut self) {
        let _ = self.child.start_kill();
    }
}

impl Drop for Running {
    fn drop(&mut self) {
        if let Some(dir) = &self.cgroup {
            // 프로세스가 다 빠진 빈 cgroup만 지워짐
            let _ = std::fs::remove_dir(dir);
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Watch {
    Fine,
    Warn,
    OverLimit,
    Killed,
    Exited(Option<i32>),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ram_sizes() {
        assert_eq!(parse_bytes("4G"), Some(4 << 30));
        assert_eq!(parse_bytes("512m"), Some(512 << 20));
        assert_eq!(parse_bytes("1.5GiB"), Some(3 << 29));
        assert_eq!(parse_bytes("2048"), Some(2048));
        assert_eq!(parse_bytes("lots"), None);
        assert_eq!(format_bytes(4 << 30), "4G");
        assert_eq!(format_bytes(512 << 20), "512M");
        let c = InstanceConfig {
            ram: Some(6 << 30),
            seed: Some(-5),
            ..InstanceConfig::new("rpg", 25603)
        };
        assert_eq!(InstanceConfig::from_json(&c.to_json()), Some(c));
    }

    #[test]
    fn reads_own_rss() {
        if cfg!(target_os = "linux") {
            assert!(rss_of(std::process::id()).unwrap() > 1 << 20);
        }
    }
}
