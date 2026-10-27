use std::io::{self, Read};

use memchr::memmem::Finder;

use crate::{
    errors::MatcherError,
    utils::{Segment, compile_segments},
};

const STREAM_CHUNK: usize = 16 * 1024;

/// Compiled `LIKE` pattern, cached across rows through SQLite auxdata.
pub enum Matcher {
    /// No `_`: lowercase literals that must appear in order.
    Ordered(Vec<Finder<'static>>),
    /// At least one `_`: segments matched over the buffered stream.
    WithAny(Vec<Segment>),
}

impl Matcher {
    pub fn compile(pattern: &[u8]) -> Self {
        let segments = compile_segments(pattern);
        if segments.iter().any(|s| matches!(s, Segment::Any)) {
            return Self::WithAny(segments);
        }

        let finders = segments
            .into_iter()
            .filter_map(|s| match s {
                Segment::Literal(f) => Some(*f),
                _ => None,
            })
            .collect();
        Self::Ordered(finders)
    }

    pub fn find_in<R: Read>(&self, reader: R, skip: usize) -> Result<bool, MatcherError> {
        match self {
            Self::Ordered(finders) => fast_path_ordered_segments(reader, finders, skip),
            Self::WithAny(segments) => slow_path_with_any(reader, segments, skip),
        }
    }
}

/// Finds each literal after the end of the previous one, over a byte stream.
///
/// Leftmost matching is optimal for `%`-separated literals, so no
/// backtracking is needed. The window keeps `len - 1` bytes of the current
/// literal's prefix so matches spanning chunks are found.
fn fast_path_ordered_segments<R: Read>(
    mut reader: R,
    finders: &[Finder<'static>],
    mut skip: usize,
) -> Result<bool, MatcherError> {
    let Some(longest) = finders.iter().map(|f| f.needle().len()).max() else {
        return Ok(true);
    };

    let mut buf = vec![0u8; longest - 1 + STREAM_CHUNK];
    let mut kept = 0;
    let mut current = 0;

    loop {
        let n = reader.read(&mut buf[kept..kept + STREAM_CHUNK])?;
        if n == 0 {
            return Ok(false);
        }
        let mut end = kept + n;

        if skip > 0 {
            if skip >= n {
                skip -= n;
                continue;
            }
            buf.copy_within(kept + skip..end, kept);
            end -= skip;
            skip = 0;
        }

        buf[kept..end].make_ascii_lowercase();

        let mut pos = 0;
        while let Some(i) = finders[current].find(&buf[pos..end]) {
            pos += i + finders[current].needle().len();
            current += 1;
            if current == finders.len() {
                return Ok(true);
            }
        }

        let keep = (finders[current].needle().len() - 1).min(end - pos);
        buf.copy_within(end - keep..end, 0);
        kept = keep;
    }
}

/// Matches each `%`-separated group at its leftmost position, using a bounded
/// buffer that retains only bytes needed by the current group.
///
/// The leftmost match of a group leaves the most room for the next ones, so
/// backtracking never crosses a `Skip`: on mismatch, only the group's start
/// moves forward, to the next hit of its first literal.
fn slow_path_with_any<R: Read>(
    mut reader: R,
    segments: &[Segment],
    mut skip: usize,
) -> Result<bool, MatcherError> {
    let mut buf = Vec::new();
    let mut eof = false;
    let mut group_start = 0;
    let mut pos = 0;

    loop {
        let group_end = segments[group_start..]
            .iter()
            .position(|s| matches!(s, Segment::Skip))
            .map_or(segments.len(), |i| group_start + i);

        match find_group(&segments[group_start..group_end], &buf, pos, eof) {
            Group::Found(end) => {
                if group_end == segments.len() {
                    return Ok(true);
                }
                group_start = group_end + 1;
                pos = end;
            }
            Group::Missing => return Ok(false),
            Group::Pending(from) => {
                buf.drain(..from);
                pos = 0;
                eof = !read_more(&mut reader, &mut buf, &mut skip)?;
            }
        }
    }
}

/// Appends the next bytes past `skip`, lowercased; returns `false` at EOF.
fn read_more<R: Read>(reader: &mut R, buf: &mut Vec<u8>, skip: &mut usize) -> io::Result<bool> {
    loop {
        let len = buf.len();
        buf.resize(len + STREAM_CHUNK, 0);
        let n = reader.read(&mut buf[len..])?;
        let dropped = n.min(*skip);
        *skip -= dropped;
        buf.copy_within(len + dropped..len + n, len);
        buf.truncate(len + n - dropped);

        if n == 0 {
            return Ok(false);
        }
        if n > dropped {
            buf[len..].make_ascii_lowercase();
            return Ok(true);
        }
    }
}

enum Group {
    /// Matched, ending at this offset.
    Found(usize),
    /// No match starts before this offset, more input is needed past it.
    Pending(usize),
    Missing,
}

/// Finds the leftmost match of a `Skip`-free group starting at or after `from`.
fn find_group(group: &[Segment], hay: &[u8], mut from: usize, eof: bool) -> Group {
    loop {
        let start = match &group[0] {
            Segment::Literal(f) => match f.find(&hay[from..]) {
                Some(i) => from + i,
                None if eof => return Group::Missing,
                None => {
                    let tail = hay.len().saturating_sub(f.needle().len() - 1);
                    return Group::Pending(tail.max(from));
                }
            },
            _ if from < hay.len() => from,
            _ if eof => return Group::Missing,
            _ => return Group::Pending(from),
        };

        match match_at(group, hay, start, eof) {
            Step::Match(end) => return Group::Found(end),
            Step::Mismatch => from = start + 1,
            Step::NeedMore => return Group::Pending(start),
        }
    }
}

enum Step {
    Match(usize),
    Mismatch,
    NeedMore,
}

/// Matches a `Skip`-free group anchored at `pos`.
fn match_at(group: &[Segment], hay: &[u8], mut pos: usize, eof: bool) -> Step {
    for segment in group {
        let rest = &hay[pos..];
        match segment {
            Segment::Literal(f) => {
                let needle = f.needle();
                if rest.len() < needle.len() {
                    return if !eof && needle.starts_with(rest) {
                        Step::NeedMore
                    } else {
                        Step::Mismatch
                    };
                }
                if !rest.starts_with(needle) {
                    return Step::Mismatch;
                }
                pos += needle.len();
            }
            Segment::Any => match any_len(rest, eof) {
                Some(0) => return Step::Mismatch,
                Some(n) => pos += n,
                None => return Step::NeedMore,
            },
            Segment::Skip => unreachable!("groups are split on Skip"),
        }
    }
    Step::Match(pos)
}

/// Bytes taken by one `_` at the start of `hay`: a whole UTF-8 character, or
/// a single byte when the sequence is invalid. `None` when more input could
/// change the answer.
fn any_len(hay: &[u8], eof: bool) -> Option<usize> {
    let Some(&lead) = hay.first() else {
        return if eof { Some(0) } else { None };
    };
    let width = match lead {
        0xC2..=0xDF => 2,
        0xE0..=0xEF => 3,
        0xF0..=0xF4 => 4,
        _ => return Some(1),
    };
    match std::str::from_utf8(&hay[..width.min(hay.len())]) {
        Ok(_) => Some(width),
        Err(e) if e.error_len().is_none() && !eof => None,
        Err(_) => Some(1),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn matches(pattern: &str, haystack: &[u8]) -> bool {
        Matcher::compile(pattern.as_bytes())
            .find_in(haystack, 0)
            .unwrap()
    }

    /// Feeds one byte per `read` to exercise matches spanning chunks.
    struct Trickle<'a>(&'a [u8]);

    impl Read for Trickle<'_> {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            let Some((&b, rest)) = self.0.split_first() else {
                return Ok(0);
            };
            buf[0] = b;
            self.0 = rest;
            Ok(1)
        }
    }

    #[test]
    fn literals_are_case_insensitive_and_ordered() {
        assert!(matches("hello", b"say HeLLo world"));
        assert!(matches("say%WORLD", b"say hello world"));
        assert!(!matches("world%say", b"say hello world"));
        assert!(matches("", b"anything"));
        assert!(matches("%%", b""));
        assert!(!matches("x", b""));
    }

    #[test]
    fn underscore_matches_one_character() {
        assert!(matches("h_llo", b"xx HALLO yy"));
        assert!(matches("a_c", "aéc".as_bytes()));
        assert!(!matches("a__c", "aéc".as_bytes()));
        assert!(matches("a_c", b"a\xFFc"));
        assert!(matches("a_c", b"a\xE2c"));
        assert!(!matches("a_c", b"ac"));
        assert!(matches("_", b"z"));
        assert!(!matches("_", b""));
        assert!(matches("a_%z", b"ab----Z"));
        assert!(matches("%_%", b"z"));
        assert!(!matches("a_", b"a"));
    }

    #[test]
    fn backtracks_to_next_literal_hit() {
        assert!(matches("test_oui", b"testXtestYoui"));
        assert!(matches("te%test_oui", b"te---testX---te---testYoui"));
        assert!(!matches("te%test_oui", b"te---testX---testYnon"));
        assert!(matches("a_c", b"axxa_c"));
        assert!(matches("aa_b", b"aaaxb"));
        assert!(matches("_%z", b"..z"));
        assert!(!matches("_%z", b"z"));
        assert!(matches("_b", b"aab"));
    }

    #[test]
    fn matches_span_chunks() {
        for pattern in ["needle%tail", "ne_dle%tail"] {
            let m = Matcher::compile(pattern.as_bytes());
            assert!(m.find_in(Trickle(b"..NEEDLE..TAIL.."), 0).unwrap());
            assert!(!m.find_in(Trickle(b"..TAIL..NEEDLE.."), 0).unwrap());
        }
        let m = Matcher::compile(b"a_c");
        assert!(m.find_in(Trickle("xaéc".as_bytes()), 0).unwrap());
        assert!(!m.find_in(Trickle("xaé".as_bytes()), 0).unwrap());
    }

    #[test]
    fn offset_skips_prefix() {
        for pattern in ["abc", "a_c"] {
            let m = Matcher::compile(pattern.as_bytes());
            assert!(m.find_in(&b"abc---abc"[..], 3).unwrap());
            assert!(!m.find_in(&b"abc------"[..], 1).unwrap());
        }
    }
}
