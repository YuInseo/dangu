//! 퀘스트: 일반 · 일일 · 주간 · 길드.
//!
//! | 종류 | 받는 법 | 초기화 | 보상 받는 사람 |
//! |---|---|---|---|
//! | 일반 | `/퀘스트` 창에서 수락 | 없음 (`repeatable`이면 끝난 뒤 다시 수락) | 나 |
//! | 주간 | 저절로 (그 주의 퀘스트 `weekly_count`개) | 월요일 0시 (한국) | 나 |
//! | 길드 주간 | 저절로, 길드원이 함께 채움 | 월요일 0시 | 길드(GP) + 끝낸 사람 |
//! | 일일·주간·길드 납품 | 저절로 자원 1~3가지 (주간 ×3, 길드 ×2) | 매일 / 월요일 / 매일 | 모두 내면 자동심기·플라이 (길드는 낸 길드원 모두) |
//!
//! 퀘스트 내용은 설정 파일(`MC_QUESTS`, 기본 `world/quests.json`)에 있고 웹 관리 페이지에서
//! 고칩니다. 진행은 `world/quest_progress.json` (모든 인스턴스가 같이 씀).
//!
//! 목표: `mine`(블록 캐기), `ores`(광석), `logs`(통나무), `harvest`(다 자란 작물), `kill`(몹),
//! `deliver`(아이템 내기 — 창에서 누르면 가방에서 가져감). `filter`가 있으면 그 이름만
//! (`*_ore`처럼 별표 가능).

pub mod config;
pub mod gui;
pub mod progress;
mod session;

pub use config::{QuestConfig, QuestDef};
pub use progress::{Happened, HudQuest, Kind};
pub use session::{open_global, progress};

/// 한국 시각 날 번호.
pub fn day_of(now: i64) -> i64 {
    (now + 9 * 3600).div_euclid(86_400)
}

/// 주 번호 (월요일 0시에 바뀜; 1970-01-01은 목요일).
pub fn week_of(now: i64) -> i64 {
    (day_of(now) + 3).div_euclid(7)
}

pub fn now() -> i64 {
    crate::plugins::guild::now()
}

/// 별표 이름 맞추기 (`*_ore`, `oak_*`, `*`).
pub fn name_matches(pattern: &str, name: &str) -> bool {
    let pattern = pattern.strip_prefix("minecraft:").unwrap_or(pattern);
    match pattern.split_once('*') {
        None => pattern == name,
        Some((pre, post)) => {
            name.len() >= pre.len() + post.len() && name.starts_with(pre) && name.ends_with(post)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn weeks_and_patterns() {
        // 2026-10-19 (월) 00:00 KST
        let monday = 1_792_335_600;
        assert_eq!(week_of(monday), week_of(monday + 6 * 86_400 + 86_399));
        assert_ne!(week_of(monday - 1), week_of(monday));
        assert!(name_matches("*_ore", "iron_ore"));
        assert!(name_matches("minecraft:oak_*", "oak_log"));
        assert!(name_matches("*", "anything"));
        assert!(!name_matches("*_ore", "iron_block"));
    }
}
