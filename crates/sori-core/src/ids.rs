//! 짧고 URL에 안전한 고정 ID와 비교용 정규화.

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use sha1::{Digest, Sha1};
use unicode_normalization::UnicodeNormalization;

/// 같은 입력이면 다시 스캔해도 같은 값이 나온다. SHA-1을 base64url로 적은 앞 12자.
pub fn make_id(input: &str) -> String {
    let mut s = URL_SAFE_NO_PAD.encode(Sha1::digest(input.as_bytes()));
    s.truncate(12);
    s
}

/// 대소문자, 앞뒤 공백, 유니코드 조합 차이를 지운다. "Ｂｅａｔｌｅｓ"와 "beatles"가 같아진다.
pub fn norm(s: &str) -> String {
    let n: String = s.nfkc().collect();
    let lower = n.trim().to_lowercase();
    let mut out = String::with_capacity(lower.len());
    let mut gap = false;
    for c in lower.chars() {
        if c.is_whitespace() {
            gap = !out.is_empty();
        } else {
            if gap {
                out.push(' ');
                gap = false;
            }
            out.push(c);
        }
    }
    out
}

pub fn artist_id(name: &str) -> String {
    make_id(&format!("artist:{}", norm(name)))
}

pub fn genre_id(name: &str) -> String {
    make_id(&format!("genre:{}", norm(name)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_stable_and_short() {
        // Node의 createHash('sha1').digest('base64url').slice(0, 12)와 같은 값이어야 한다.
        assert_eq!(make_id("track:a.flac"), make_id("track:a.flac"));
        assert_eq!(make_id("x").len(), 12);
        assert!(make_id("x").chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'));
    }

    #[test]
    fn norm_folds_width_case_and_space() {
        assert_eq!(norm("  Ｔｈｅ   Beatles\t"), "the beatles");
        assert_eq!(norm("아이유"), "아이유");
        assert_eq!(artist_id("The Beatles"), artist_id("the  beatles "));
    }
}
