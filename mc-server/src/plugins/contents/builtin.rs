//! 서버에 들어 있는 콘텐츠 팩들. 디스크의 `contents/` 팩보다 먼저 읽히고, 같은 파일이 있으면
//! 디스크 쪽이 이깁니다. 각 플러그인이 자기 팩을 만들어 여기서 모읍니다.

use super::source::Pack;

pub fn packs() -> Vec<Pack> {
    let mut packs = vec![crate::plugins::lottery::art::pack()];
    if crate::plugins::hud::enabled() {
        packs.push(crate::plugins::hud::art::pack());
    }
    packs
}
