//! 연결들이 함께 보는 서버 상태.

use std::sync::atomic::{AtomicI32, AtomicU32, Ordering};

use tokio::sync::broadcast;

use crate::packets::status::ServerInfo;
use crate::protocol::nbt::Tag;

pub struct Server {
    pub info: ServerInfo,
    /// 월드 시드. 세 차원 모두 이 값에서 지형을 만듭니다.
    pub seed: u64,
    online: AtomicU32,
    next_entity_id: AtomicI32,
    /// 모든 접속자에게 보낼 시스템 채팅.
    chat: broadcast::Sender<Tag>,
}

impl Server {
    pub fn new(info: ServerInfo, seed: u64) -> Self {
        Self {
            info,
            seed,
            online: AtomicU32::new(0),
            // 26.2부터 클라이언트가 엔티티 ID 0을 거부합니다.
            next_entity_id: AtomicI32::new(1),
            chat: broadcast::channel(256).0,
        }
    }

    pub fn online(&self) -> u32 {
        self.online.load(Ordering::Relaxed)
    }

    /// 접속 인원을 하나 늘리고, 떨어뜨리면 다시 줄이는 표를 돌려줍니다.
    pub fn join(&self) -> OnlineGuard<'_> {
        self.online.fetch_add(1, Ordering::Relaxed);
        OnlineGuard(self)
    }

    pub fn allocate_entity_id(&self) -> i32 {
        self.next_entity_id.fetch_add(1, Ordering::Relaxed)
    }

    pub fn broadcast(&self, message: Tag) {
        // 받는 사람이 없으면 Err인데, 그건 문제가 아닙니다.
        let _ = self.chat.send(message);
    }

    pub fn subscribe_chat(&self) -> broadcast::Receiver<Tag> {
        self.chat.subscribe()
    }
}

pub struct OnlineGuard<'a>(&'a Server);

impl Drop for OnlineGuard<'_> {
    fn drop(&mut self) {
        self.0.online.fetch_sub(1, Ordering::Relaxed);
    }
}
