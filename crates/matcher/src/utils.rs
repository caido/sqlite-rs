use memchr::memmem::Finder;

/// One piece of a compiled `LIKE` pattern.
pub enum Segment {
    /// ASCII-lowercased bytes that must appear verbatim.
    Literal(Finder<'static>),
    /// `%`: any run of bytes, possibly empty.
    Skip,
    /// `_`: one UTF-8 character, or one byte when the sequence is invalid.
    Any,
}

/// Splits a `LIKE` pattern into segments.
///
/// Runs of `%` collapse into one `Skip`, and leading or trailing `Skip`s are
/// dropped since matching is unanchored on both ends.
pub(crate) fn compile_segments(pattern: &[u8]) -> Vec<Segment> {
    let mut segments = Vec::new();
    let mut cur = Vec::new();
    let mut i = 0;
    while i < pattern.len() {
        match pattern[i] {
            b'\\' if i + 1 < pattern.len() => {
                cur.push(pattern[i + 1]);
                i += 2;
            }
            b'%' => {
                push_literal(&mut segments, &mut cur);
                if !segments.is_empty() && !matches!(segments.last(), Some(Segment::Skip)) {
                    segments.push(Segment::Skip);
                }
                i += 1;
            }
            b'_' => {
                push_literal(&mut segments, &mut cur);
                segments.push(Segment::Any);
                i += 1;
            }
            b => {
                cur.push(b);
                i += 1;
            }
        }
    }
    push_literal(&mut segments, &mut cur);
    if matches!(segments.last(), Some(Segment::Skip)) {
        segments.pop();
    }
    segments
}

fn push_literal(segments: &mut Vec<Segment>, cur: &mut Vec<u8>) {
    if cur.is_empty() {
        return;
    }
    cur.make_ascii_lowercase();
    segments.push(Segment::Literal(Finder::new(cur).into_owned()));
    cur.clear();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn segs(p: &str) -> Vec<String> {
        compile_segments(p.as_bytes())
            .iter()
            .map(|s| match s {
                Segment::Literal(f) => String::from_utf8_lossy(f.needle()).into_owned(),
                Segment::Skip => "<skip>".into(),
                Segment::Any => "<any>".into(),
            })
            .collect()
    }

    #[test]
    fn splits_on_wildcards() {
        assert_eq!(
            segs("te%test_oui"),
            ["te", "<skip>", "test", "<any>", "oui"]
        );
        assert_eq!(segs("a_%z"), ["a", "<any>", "<skip>", "z"]);
        assert_eq!(segs("__"), ["<any>", "<any>"]);
    }

    #[test]
    fn normalizes_skips() {
        assert_eq!(segs(""), [] as [&str; 0]);
        assert_eq!(segs("%%"), [] as [&str; 0]);
        assert_eq!(segs("%abc%"), ["abc"]);
        assert_eq!(segs("a%%b"), ["a", "<skip>", "b"]);
        assert_eq!(segs("_abc_%"), ["<any>", "abc", "<any>"]);
    }

    #[test]
    fn lowercases_ascii_literals() {
        assert_eq!(segs("SaY%WÉrld"), ["say", "<skip>", "wÉrld"]);
    }

    #[test]
    fn unescapes() {
        assert_eq!(segs(r"foo\%bar"), ["foo%bar"]);
        assert_eq!(segs(r"a\%b%c"), ["a%b", "<skip>", "c"]);
        assert_eq!(segs(r"\_"), ["_"]);
        assert_eq!(segs(r"\\"), ["\\"]);
        assert_eq!(segs(r"\\%x"), ["\\", "<skip>", "x"]);
        assert_eq!(segs(r"\\_"), ["\\", "<any>"]);
        assert_eq!(segs(r"a\xb"), ["axb"]);
        assert_eq!(segs(r"trail\"), ["trail\\"]);
    }
}
