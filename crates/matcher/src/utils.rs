use std::fmt::Write as _;

/// Translates a `LIKE` pattern into a byte-oriented regex.
///
/// Literals are emitted as `\xHH` so ASCII folds case and other bytes match
/// verbatim. `_` is one UTF-8 character, or one byte that cannot start a
/// valid sequence so invalid UTF-8 still advances.
pub fn like_to_regex(pattern: &[u8]) -> String {
    let mut re = String::with_capacity(8 + pattern.len() * 4);
    re.push_str("(?is-u)");
    let mut i = 0;
    while i < pattern.len() {
        match pattern[i] {
            b'\\' if i + 1 < pattern.len() => {
                let _ = write!(re, r"\x{:02X}", pattern[i + 1]);
                i += 2;
            }
            b'\\' => {
                let _ = write!(re, r"\x{:02X}", b'\\');
                i += 1;
            }
            b'%' => {
                re.push_str(".*");
                i += 1;
            }
            b'_' => {
                re.push_str(r"(?:(?u-i:.)|[\x80-\xBF\xC0\xC1\xF5-\xFF])");
                i += 1;
            }
            b => {
                let _ = write!(re, r"\x{b:02X}");
                i += 1;
            }
        }
    }
    re
}

pub fn like_literal_segments(pattern: &[u8]) -> Option<Vec<Vec<u8>>> {
    let mut segments = Vec::new();
    let mut cur = Vec::new();
    let mut i = 0;
    while i < pattern.len() {
        match pattern[i] {
            b'\\' if i + 1 < pattern.len() => {
                cur.push(pattern[i + 1]);
                i += 2;
            }
            b'\\' => {
                cur.push(b'\\');
                i += 1;
            }
            b'%' => {
                if !cur.is_empty() {
                    segments.push(std::mem::take(&mut cur));
                }
                i += 1;
            }
            b'_' => return None,
            b => {
                cur.push(b);
                i += 1;
            }
        }
    }
    if !cur.is_empty() {
        segments.push(cur);
    }
    Some(segments)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn segs(p: &str) -> Option<Vec<Vec<u8>>> {
        like_literal_segments(p.as_bytes())
    }

    #[test]
    fn literal_segments_split_on_unescaped_percent() {
        assert_eq!(segs("hello"), Some(vec![b"hello".to_vec()]));
        assert_eq!(
            segs("say%WORLD"),
            Some(vec![b"say".to_vec(), b"WORLD".to_vec()])
        );
        assert_eq!(segs("%%"), Some(vec![]));
        assert_eq!(segs("%abc%"), Some(vec![b"abc".to_vec()]));
        assert_eq!(segs(""), Some(vec![]));
    }

    #[test]
    fn literal_segments_unescape() {
        assert_eq!(segs(r"foo\%bar"), Some(vec![b"foo%bar".to_vec()]));
        assert_eq!(segs(r"a\%b%c"), Some(vec![b"a%b".to_vec(), b"c".to_vec()]));
        assert_eq!(segs(r"\_"), Some(vec![b"_".to_vec()])); // échappé → littéral, pas DFA
        assert_eq!(segs(r"\\"), Some(vec![b"\\".to_vec()]));
        assert_eq!(segs(r"\\%x"), Some(vec![b"\\".to_vec(), b"x".to_vec()]));
        assert_eq!(segs(r"a\xb"), Some(vec![b"axb".to_vec()])); // `\x` → `x`
        assert_eq!(segs(r"trail\"), Some(vec![b"trail\\".to_vec()]));
    }

    #[test]
    fn literal_segments_none_on_unescaped_underscore() {
        assert_eq!(segs("a_c"), None);
        assert_eq!(segs(r"a\_c_d"), None); // un `_` libre suffit
        assert_eq!(segs(r"\\_"), None); // `\\` → `\`, puis `_` wildcard
    }

    #[test]
    fn like_to_regex_escapes() {
        assert_eq!(
            like_to_regex(br"foo\%bar"),
            r"(?is-u)\x66\x6F\x6F\x25\x62\x61\x72"
        );
        assert_eq!(like_to_regex(br"a\%b%c"), r"(?is-u)\x61\x25\x62.*\x63");
        assert_eq!(like_to_regex(br"\_"), r"(?is-u)\x5F");
        assert_eq!(like_to_regex(br"\\"), r"(?is-u)\x5C");
        assert_eq!(like_to_regex(br"\\%"), r"(?is-u)\x5C.*");
        let re = like_to_regex(br"\\_");
        assert!(re.starts_with(r"(?is-u)\x5C"));
        assert!(re.contains(r"(?:(?u-i:.)|"));
        assert!(!re.contains(r"\x5F")); // `_` n'est pas littéral
    }
}
