//! 사람이 기대하는 정렬. `Intl.Collator('ko', { numeric: true, sensitivity: 'base' })`를 흉내 낸다.
//!
//! - 숫자는 수로 비교한다: "Track 2" < "Track 10".
//! - 대소문자와 악센트는 보지 않는다: "Émile" == "emile".
//! - 한글은 유니코드 순서가 곧 가나다순이다.
//! - 라틴 글자가 한글보다 앞에 온다.

use std::cmp::Ordering;
use unicode_normalization::UnicodeNormalization;
use unicode_normalization::char::is_combining_mark;

fn fold(s: &str) -> Vec<char> {
    s.nfd().filter(|c| !is_combining_mark(*c)).flat_map(char::to_lowercase).collect()
}

/// 글자 무리: 숫자 < 기호 < 라틴 < 그 밖(한글, 한자…)
fn class(c: char) -> u8 {
    if c.is_ascii_digit() {
        0
    } else if c.is_ascii_alphabetic() {
        2
    } else if c.is_alphanumeric() {
        3
    } else {
        1
    }
}

pub fn compare(a: &str, b: &str) -> Ordering {
    let (a, b) = (fold(a), fold(b));
    let (mut i, mut j) = (0, 0);
    while i < a.len() && j < b.len() {
        let (ca, cb) = (a[i], b[j]);
        if ca.is_ascii_digit() && cb.is_ascii_digit() {
            let si = i;
            while i < a.len() && a[i].is_ascii_digit() {
                i += 1;
            }
            let sj = j;
            while j < b.len() && b[j].is_ascii_digit() {
                j += 1;
            }
            let na: String = a[si..i].iter().collect::<String>().trim_start_matches('0').to_string();
            let nb: String = b[sj..j].iter().collect::<String>().trim_start_matches('0').to_string();
            let ord = na.len().cmp(&nb.len()).then_with(|| na.cmp(&nb));
            if ord != Ordering::Equal {
                return ord;
            }
            continue;
        }
        if ca != cb {
            let (ka, kb) = (class(ca), class(cb));
            if ka != kb {
                return ka.cmp(&kb);
            }
            return ca.cmp(&cb);
        }
        i += 1;
        j += 1;
    }
    (a.len() - i).cmp(&(b.len() - j))
}

#[cfg(test)]
mod tests {
    use super::compare;
    use std::cmp::Ordering::*;

    #[test]
    fn numeric_and_case() {
        assert_eq!(compare("Track 2", "track 10"), Less);
        assert_eq!(compare("ABBA", "abba"), Equal);
        assert_eq!(compare("Émile", "emile"), Equal);
        assert_eq!(compare("가나다", "가다"), Less);
        assert_eq!(compare("Zebra", "가"), Less);
        assert_eq!(compare("01 Intro", "1 Intro"), Equal);
    }
}
