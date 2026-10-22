//! 서명된 채팅 (1.19.3~의 "보안 채팅").
//!
//! 1. 정품 클라이언트는 접속 뒤 자기 채팅 키(RSA)와, Mojang이 그 키에 한 서명을 보냅니다
//!    (`Chat Session Update`). 서버는 Mojang 공개 키로 그 서명을 확인합니다.
//! 2. 채팅마다 클라이언트는 (보낸 사람, 세션, 순번, 소금, 시각, 내용, 자기가 읽은 서명된
//!    메시지들)을 SHA-256 RSA로 서명합니다.
//! 3. 서버는 그 서명을 확인한 뒤 서명째 모두에게 전합니다 (`Player Chat`). 받는 클라이언트도
//!    같은 계산으로 확인하므로, 서버는 "자기가 읽은 메시지" 목록을 클라이언트와 똑같이
//!    재구성해야 합니다 — 그게 [`LastSeenValidator`]입니다.
//!
//! 확인에 실패하면 서명 없는 채팅으로 내려 보냅니다 (보이기는 하되 "보안 아님" 표시).

use crate::crypto::rsa::PublicKey;

pub const SIGNATURE_LEN: usize = 256;
/// 클라이언트가 기억하는 최근 서명된 메시지 수.
pub const LAST_SEEN_WINDOW: usize = 20;

pub type Signature = [u8; SIGNATURE_LEN];

/// 클라이언트의 채팅 세션.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatSession {
    pub session_id: u128,
    /// 키 만료 시각 (밀리초).
    pub expires_at: i64,
    /// X.509 DER 공개 키.
    pub key_der: Vec<u8>,
    /// Mojang 서명 (SHA1withRSA).
    pub key_signature: Vec<u8>,
}

impl ChatSession {
    pub fn public_key(&self) -> Option<PublicKey> {
        PublicKey::from_der(&self.key_der)
    }

    /// Mojang이 "이 사람(uuid)의 이 키, 이 만료 시각"에 서명했는지.
    pub fn is_signed_by(&self, player: u128, mojang: &[PublicKey], now_ms: i64) -> bool {
        if self.expires_at <= now_ms {
            return false;
        }
        let mut payload = Vec::with_capacity(24 + self.key_der.len());
        payload.extend_from_slice(&player.to_be_bytes());
        payload.extend_from_slice(&self.expires_at.to_be_bytes());
        payload.extend_from_slice(&self.key_der);
        mojang
            .iter()
            .any(|k| k.verify_sha1(&payload, &self.key_signature))
    }
}

/// 서명 대상 바이트: 버전 1, 링크(보낸 사람, 세션, 순번), 본문(소금, 시각(초), 내용, 읽은 메시지).
pub fn signed_payload(
    sender: u128,
    session_id: u128,
    index: i32,
    salt: i64,
    timestamp_ms: i64,
    content: &str,
    last_seen: &[Signature],
) -> Vec<u8> {
    let mut p = Vec::with_capacity(64 + content.len() + last_seen.len() * SIGNATURE_LEN);
    p.extend_from_slice(&1i32.to_be_bytes());
    p.extend_from_slice(&sender.to_be_bytes());
    p.extend_from_slice(&session_id.to_be_bytes());
    p.extend_from_slice(&index.to_be_bytes());
    p.extend_from_slice(&salt.to_be_bytes());
    p.extend_from_slice(&timestamp_ms.div_euclid(1000).to_be_bytes());
    p.extend_from_slice(&(content.len() as i32).to_be_bytes());
    p.extend_from_slice(content.as_bytes());
    p.extend_from_slice(&(last_seen.len() as i32).to_be_bytes());
    for s in last_seen {
        p.extend_from_slice(s);
    }
    p
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Tracked {
    signature: Signature,
    pending: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LastSeenError {
    BadOffset,
    MissingAcknowledged,
    LostAcknowledged,
}

/// 서버가 이 클라이언트에 보낸 서명된 메시지들과, 클라이언트가 "읽었다"고 한 것.
/// 바닐라 `LastSeenMessagesValidator`와 같은 규칙입니다.
#[derive(Debug, Clone)]
pub struct LastSeenValidator {
    tracked: Vec<Option<Tracked>>,
    last_pending: Option<Signature>,
}

impl Default for LastSeenValidator {
    fn default() -> Self {
        Self {
            tracked: vec![None; LAST_SEEN_WINDOW],
            last_pending: None,
        }
    }
}

impl LastSeenValidator {
    /// 서명된 메시지를 이 클라이언트에 보낼 때마다.
    pub fn add_pending(&mut self, signature: Signature) {
        if self.last_pending != Some(signature) {
            self.tracked.push(Some(Tracked {
                signature,
                pending: true,
            }));
            self.last_pending = Some(signature);
        }
    }

    pub fn pending_count(&self) -> usize {
        self.tracked.len()
    }

    /// 창을 `offset`만큼 앞으로 (오래된 것 버리기).
    pub fn apply_offset(&mut self, offset: i32) -> Result<(), LastSeenError> {
        let room = self.tracked.len() - LAST_SEEN_WINDOW;
        match usize::try_from(offset) {
            Ok(o) if o <= room => {
                self.tracked.drain(..o);
                Ok(())
            }
            _ => Err(LastSeenError::BadOffset),
        }
    }

    /// 채팅에 실려 온 (offset, 읽음 비트 20개)로, 서명에 들어간 "읽은 메시지" 목록을 만듭니다.
    pub fn apply_update(
        &mut self,
        offset: i32,
        acknowledged: u32,
    ) -> Result<Vec<Signature>, LastSeenError> {
        self.apply_offset(offset)?;
        let mut seen = Vec::new();
        for i in 0..LAST_SEEN_WINDOW {
            let entry = self.tracked[i];
            if acknowledged & (1 << i) != 0 {
                let e = entry.ok_or(LastSeenError::MissingAcknowledged)?;
                self.tracked[i] = Some(Tracked {
                    pending: false,
                    ..e
                });
                seen.push(e.signature);
            } else {
                if entry.is_some_and(|e| !e.pending) {
                    return Err(LastSeenError::LostAcknowledged);
                }
                self.tracked[i] = None;
            }
        }
        Ok(seen)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::rsa::PrivateKey;

    fn sig(b: u8) -> Signature {
        [b; SIGNATURE_LEN]
    }

    #[test]
    fn last_seen_window_like_vanilla() {
        let mut v = LastSeenValidator::default();
        // 아무것도 안 읽음
        assert_eq!(v.apply_update(0, 0), Ok(vec![]));
        v.add_pending(sig(1));
        v.add_pending(sig(1)); // 같은 것은 한 번만
        assert_eq!(v.pending_count(), 21);
        // 클라이언트: 창을 1 밀고, 맨 끝(19번)을 읽음
        assert_eq!(v.apply_update(1, 1 << 19), Ok(vec![sig(1)]));
        v.add_pending(sig(2));
        // 다시 1 밀면 sig1은 18번, sig2는 19번
        assert_eq!(
            v.apply_update(1, (1 << 18) | (1 << 19)),
            Ok(vec![sig(1), sig(2)])
        );
        // 읽었던 것을 안 읽었다고 하면 오류
        assert_eq!(
            v.apply_update(0, 1 << 19),
            Err(LastSeenError::LostAcknowledged)
        );
    }

    #[test]
    fn bad_offsets_and_missing_entries() {
        let mut v = LastSeenValidator::default();
        assert_eq!(v.apply_offset(1), Err(LastSeenError::BadOffset));
        assert_eq!(
            v.apply_update(0, 1),
            Err(LastSeenError::MissingAcknowledged)
        );
    }

    #[test]
    fn mojang_key_signature() {
        let mojang = PrivateKey::generate(1024);
        let player_key = PrivateKey::generate(512);
        let player = 0x1234u128;
        let expires_at = 2_000_000_000_000i64;
        let key_der = player_key.public.to_der();
        let mut payload = player.to_be_bytes().to_vec();
        payload.extend_from_slice(&expires_at.to_be_bytes());
        payload.extend_from_slice(&key_der);
        let session = ChatSession {
            session_id: 9,
            expires_at,
            key_der,
            key_signature: mojang.sign_sha1(&payload),
        };
        let keys = [mojang.public.clone()];
        assert!(session.is_signed_by(player, &keys, 0));
        assert!(
            !session.is_signed_by(player + 1, &keys, 0),
            "someone else's key"
        );
        assert!(!session.is_signed_by(player, &keys, expires_at), "expired");
    }

    #[test]
    fn payload_layout() {
        let p = signed_payload(1, 2, 3, 4, 5_999, "hi", &[sig(7)]);
        assert_eq!(&p[..4], &[0, 0, 0, 1]);
        assert_eq!(p.len(), 4 + 16 + 16 + 4 + 8 + 8 + 4 + 2 + 4 + 256);
        // 시각은 초 단위
        assert_eq!(&p[48..56], &5i64.to_be_bytes());
    }
}
