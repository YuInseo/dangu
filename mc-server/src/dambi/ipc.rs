//! 마스터 ↔ 인스턴스 프로세스 사이의 메시지 (IPC).
//!
//! 유닉스 도메인 소켓 위에 `[u32 BE 길이][한 줄 JSON]` 프레임. JSON은 `{"t":"종류", ...}`
//! 모양이라 사람이 소켓을 엿보아도 읽을 수 있고, 다른 언어로 짠 인스턴스도 붙일 수 있습니다.
//! 같은 메시지를 Redis Pub/Sub 같은 다른 버스로 실어 나르려면 [`Msg::to_json`]의 문자열을
//! 그대로 보내면 됩니다.

use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

use super::shared::SharedData;
use crate::json::{self, Json};

/// 메시지 하나의 최대 크기 (잘못된 상대가 메모리를 다 쓰지 못하게).
pub const MAX_FRAME: usize = 1 << 20;

#[derive(Debug, Clone, PartialEq)]
pub enum Msg {
    // ---- 인스턴스 → 마스터 ----
    /// 켜져서 접속을 받을 준비가 됨.
    Hello {
        instance: String,
        port: u16,
        pid: u32,
    },
    /// 5초마다: 접속자 수와 실제 메모리 사용량 (KiB).
    Status {
        online: u32,
        rss_kib: u64,
    },
    Joined {
        uuid: u128,
        name: String,
    },
    Left {
        uuid: u128,
    },
    /// 공유 데이터를 읽음. `req`로 답을 짝지음.
    SharedGet {
        req: u64,
        uuid: u128,
    },
    /// 재화를 더하거나 뺌 (`field`: money · cash · afk). 모자라면 거절됨.
    SharedAdd {
        req: u64,
        uuid: u128,
        field: String,
        delta: i64,
    },
    /// 직업 경험치.
    JobXp {
        req: u64,
        uuid: u128,
        job: String,
        xp: f64,
    },
    /// 송금: `from`이 `to`(이름)에게 돈을 보냄. 답은 보낸 사람의 `Shared`.
    Pay {
        req: u64,
        from: u128,
        to: String,
        amount: i64,
    },
    /// 이 사람을 다른 인스턴스로 보내 달라.
    TransferRequest {
        uuid: u128,
        name: String,
        to: String,
    },
    /// 전역 버스에 올림 (모든 인스턴스가 받음).
    Publish {
        channel: String,
        payload: String,
    },

    // ---- 마스터 → 인스턴스 ----
    /// `SharedGet`/`SharedAdd`/`JobXp`의 답. `ok`가 false면 바뀌지 않았음 (잔액 부족 등).
    Shared {
        req: u64,
        uuid: u128,
        ok: bool,
        data: SharedData,
    },
    /// 다른 곳에서 바뀌어 새 값을 알림.
    SharedChanged {
        uuid: u128,
        data: SharedData,
    },
    /// 이 사람을 내보내라: 클라이언트에게 Transfer 패킷을 보내면 마스터 주소로 다시 접속하고,
    /// 마스터가 `to` 인스턴스로 이어 줍니다.
    Transfer {
        uuid: u128,
        to: String,
    },
    /// 옮길 수 없음.
    Deny {
        uuid: u128,
        reason: String,
    },
    /// 콘솔 명령어를 대신 실행.
    Command {
        line: String,
    },
    /// 버스에서 온 것.
    Event {
        channel: String,
        payload: String,
    },
    /// 켜져 있는 인스턴스와 접속자 수 (`/이동` 목록용, 5초마다).
    Instances {
        list: Vec<(String, u32)>,
    },
    /// 저장하고 끝내라.
    Shutdown,
}

fn uuid_hex(u: u128) -> Json {
    Json::String(format!("{u:032x}"))
}

fn obj(t: &str, mut fields: Vec<(&str, Json)>) -> Json {
    let mut all = vec![("t".to_owned(), Json::String(t.to_owned()))];
    all.extend(fields.drain(..).map(|(k, v)| (k.to_owned(), v)));
    Json::Object(all)
}

fn s(v: &str) -> Json {
    Json::String(v.to_owned())
}

fn n(v: impl Into<f64>) -> Json {
    Json::Number(v.into())
}

impl Msg {
    pub fn to_json(&self) -> String {
        let j = match self {
            Msg::Hello {
                instance,
                port,
                pid,
            } => obj(
                "hello",
                vec![
                    ("instance", s(instance)),
                    ("port", n(*port)),
                    ("pid", n(*pid)),
                ],
            ),
            Msg::Status { online, rss_kib } => obj(
                "status",
                vec![("online", n(*online)), ("rss_kib", n(*rss_kib as f64))],
            ),
            Msg::Joined { uuid, name } => {
                obj("joined", vec![("uuid", uuid_hex(*uuid)), ("name", s(name))])
            }
            Msg::Left { uuid } => obj("left", vec![("uuid", uuid_hex(*uuid))]),
            Msg::SharedGet { req, uuid } => obj(
                "shared_get",
                vec![("req", n(*req as f64)), ("uuid", uuid_hex(*uuid))],
            ),
            Msg::SharedAdd {
                req,
                uuid,
                field,
                delta,
            } => obj(
                "shared_add",
                vec![
                    ("req", n(*req as f64)),
                    ("uuid", uuid_hex(*uuid)),
                    ("field", s(field)),
                    ("delta", n(*delta as f64)),
                ],
            ),
            Msg::JobXp { req, uuid, job, xp } => obj(
                "job_xp",
                vec![
                    ("req", n(*req as f64)),
                    ("uuid", uuid_hex(*uuid)),
                    ("job", s(job)),
                    ("xp", n(*xp)),
                ],
            ),
            Msg::Pay {
                req,
                from,
                to,
                amount,
            } => obj(
                "pay",
                vec![
                    ("req", n(*req as f64)),
                    ("from", uuid_hex(*from)),
                    ("to", s(to)),
                    ("amount", n(*amount as f64)),
                ],
            ),
            Msg::Instances { list } => obj(
                "instances",
                vec![(
                    "list",
                    Json::Array(
                        list.iter()
                            .map(|(name, online)| Json::Array(vec![s(name), n(*online)]))
                            .collect(),
                    ),
                )],
            ),
            Msg::TransferRequest { uuid, name, to } => obj(
                "transfer_request",
                vec![("uuid", uuid_hex(*uuid)), ("name", s(name)), ("to", s(to))],
            ),
            Msg::Publish { channel, payload } => obj(
                "publish",
                vec![("channel", s(channel)), ("payload", s(payload))],
            ),
            Msg::Shared {
                req,
                uuid,
                ok,
                data,
            } => obj(
                "shared",
                vec![
                    ("req", n(*req as f64)),
                    ("uuid", uuid_hex(*uuid)),
                    ("ok", Json::Bool(*ok)),
                    ("data", data.to_json()),
                ],
            ),
            Msg::SharedChanged { uuid, data } => obj(
                "shared_changed",
                vec![("uuid", uuid_hex(*uuid)), ("data", data.to_json())],
            ),
            Msg::Transfer { uuid, to } => {
                obj("transfer", vec![("uuid", uuid_hex(*uuid)), ("to", s(to))])
            }
            Msg::Deny { uuid, reason } => obj(
                "deny",
                vec![("uuid", uuid_hex(*uuid)), ("reason", s(reason))],
            ),
            Msg::Command { line } => obj("command", vec![("line", s(line))]),
            Msg::Event { channel, payload } => obj(
                "event",
                vec![("channel", s(channel)), ("payload", s(payload))],
            ),
            Msg::Shutdown => obj("shutdown", vec![]),
        };
        j.compact()
    }

    pub fn from_json(text: &str) -> Result<Msg, String> {
        let j = json::parse(text)?;
        let str_of = |k: &str| -> Result<String, String> {
            j.get(k)
                .and_then(Json::as_str)
                .map(str::to_owned)
                .ok_or_else(|| format!("'{k}' 없음"))
        };
        let num = |k: &str| -> Result<f64, String> {
            j.get(k)
                .and_then(Json::as_f64)
                .ok_or_else(|| format!("'{k}' 없음"))
        };
        let uuid = || -> Result<u128, String> {
            u128::from_str_radix(&str_of("uuid")?, 16).map_err(|e| e.to_string())
        };
        let data = || -> Result<SharedData, String> {
            SharedData::from_json(j.get("data").ok_or("'data' 없음")?)
        };
        Ok(match str_of("t")?.as_str() {
            "hello" => Msg::Hello {
                instance: str_of("instance")?,
                port: num("port")? as u16,
                pid: num("pid")? as u32,
            },
            "status" => Msg::Status {
                online: num("online")? as u32,
                rss_kib: num("rss_kib")? as u64,
            },
            "joined" => Msg::Joined {
                uuid: uuid()?,
                name: str_of("name")?,
            },
            "left" => Msg::Left { uuid: uuid()? },
            "shared_get" => Msg::SharedGet {
                req: num("req")? as u64,
                uuid: uuid()?,
            },
            "shared_add" => Msg::SharedAdd {
                req: num("req")? as u64,
                uuid: uuid()?,
                field: str_of("field")?,
                delta: num("delta")? as i64,
            },
            "job_xp" => Msg::JobXp {
                req: num("req")? as u64,
                uuid: uuid()?,
                job: str_of("job")?,
                xp: num("xp")?,
            },
            "pay" => Msg::Pay {
                req: num("req")? as u64,
                from: u128::from_str_radix(&str_of("from")?, 16).map_err(|e| e.to_string())?,
                to: str_of("to")?,
                amount: num("amount")? as i64,
            },
            "instances" => Msg::Instances {
                list: j
                    .get("list")
                    .and_then(Json::as_array)
                    .unwrap_or(&[])
                    .iter()
                    .filter_map(|e| {
                        let a = e.as_array()?;
                        Some((a.first()?.as_str()?.to_owned(), a.get(1)?.as_f64()? as u32))
                    })
                    .collect(),
            },
            "transfer_request" => Msg::TransferRequest {
                uuid: uuid()?,
                name: str_of("name")?,
                to: str_of("to")?,
            },
            "publish" => Msg::Publish {
                channel: str_of("channel")?,
                payload: str_of("payload")?,
            },
            "shared" => Msg::Shared {
                req: num("req")? as u64,
                uuid: uuid()?,
                ok: j.get("ok").and_then(Json::as_bool).unwrap_or(false),
                data: data()?,
            },
            "shared_changed" => Msg::SharedChanged {
                uuid: uuid()?,
                data: data()?,
            },
            "transfer" => Msg::Transfer {
                uuid: uuid()?,
                to: str_of("to")?,
            },
            "deny" => Msg::Deny {
                uuid: uuid()?,
                reason: str_of("reason")?,
            },
            "command" => Msg::Command {
                line: str_of("line")?,
            },
            "event" => Msg::Event {
                channel: str_of("channel")?,
                payload: str_of("payload")?,
            },
            "shutdown" => Msg::Shutdown,
            t => return Err(format!("모르는 메시지 '{t}'")),
        })
    }
}

pub async fn write_msg<W: AsyncWrite + Unpin>(w: &mut W, msg: &Msg) -> std::io::Result<()> {
    let body = msg.to_json();
    w.write_all(&(body.len() as u32).to_be_bytes()).await?;
    w.write_all(body.as_bytes()).await?;
    w.flush().await
}

/// 다음 메시지. 상대가 깔끔하게 닫았으면 `None`.
pub async fn read_msg<R: AsyncRead + Unpin>(r: &mut R) -> std::io::Result<Option<Msg>> {
    let mut len = [0u8; 4];
    match r.read_exact(&mut len).await {
        Ok(_) => {}
        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(e) => return Err(e),
    }
    let len = u32::from_be_bytes(len) as usize;
    if len > MAX_FRAME {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("IPC 메시지가 너무 큼: {len}바이트"),
        ));
    }
    let mut body = vec![0u8; len];
    r.read_exact(&mut body).await?;
    let text = String::from_utf8(body)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    Msg::from_json(&text)
        .map(Some)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn messages_round_trip_through_a_pipe() {
        let mut data = SharedData {
            money: 1500,
            ..Default::default()
        };
        data.add_job_xp("광부", 700.0);
        let msgs = vec![
            Msg::Hello {
                instance: "마을".into(),
                port: 25601,
                pid: 42,
            },
            Msg::SharedAdd {
                req: 7,
                uuid: 0xdead_beef,
                field: "cash".into(),
                delta: -300,
            },
            Msg::Shared {
                req: 7,
                uuid: u128::MAX,
                ok: true,
                data,
            },
            Msg::Event {
                channel: "공지".into(),
                payload: "서버 점검 \"10분\" 뒤".into(),
            },
            Msg::Instances {
                list: vec![("마을".into(), 3), ("rpg".into(), 0)],
            },
            Msg::Pay {
                req: 1,
                from: 5,
                to: "Steve".into(),
                amount: 250,
            },
            Msg::Shutdown,
        ];
        let (mut a, mut b) = tokio::io::duplex(4096);
        for m in &msgs {
            write_msg(&mut a, m).await.unwrap();
        }
        drop(a);
        for m in &msgs {
            assert_eq!(read_msg(&mut b).await.unwrap().as_ref(), Some(m));
        }
        assert_eq!(read_msg(&mut b).await.unwrap(), None);
    }
}
