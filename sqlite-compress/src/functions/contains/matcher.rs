use std::{
    cell::RefCell,
    io::{self, Read},
};

use memchr::memmem::Finder;
use sqlite_ffi::SqliteError;

use regex_automata::{
    Anchored,
    hybrid::dfa::{Cache, DFA},
    nfa::thompson,
    util::{start, syntax},
};

use crate::functions::contains::utils::{like_to_regex, to_sqlite};

const STREAM_CHUNK: usize = 16 * 1024;

/// Compiled `LIKE` pattern, cached across rows through SQLite auxdata.
pub(crate) enum Matcher {
    /// `%`-only pattern: lowercase literals that must appear in order.
    Literals(Vec<Finder<'static>>),
    /// Pattern with `_`: lazy DFA driven byte by byte across chunks.
    Dfa { dfa: DFA, cache: RefCell<Cache> },
}

impl Matcher {
    pub(crate) fn compile(pattern: &[u8]) -> Result<Self, SqliteError> {
        if !pattern.contains(&b'_') {
            let finders = pattern
                .split(|&b| b == b'%')
                .filter(|s| !s.is_empty())
                .map(|s| Finder::new(&s.to_ascii_lowercase()).into_owned())
                .collect();
            return Ok(Self::Literals(finders));
        }

        let dfa = DFA::builder()
            .syntax(syntax::Config::new().utf8(false))
            .thompson(thompson::Config::new().utf8(false))
            .build(&like_to_regex(pattern))
            .map_err(to_sqlite)?;
        let cache = RefCell::new(dfa.create_cache());
        Ok(Self::Dfa { dfa, cache })
    }

    pub(crate) fn find_in<R: Read>(&self, reader: R, skip: usize) -> io::Result<bool> {
        match self {
            Self::Literals(finders) => find_literals(reader, finders, skip),
            Self::Dfa { dfa, cache } => find_dfa(reader, dfa, &mut cache.borrow_mut(), skip),
        }
    }
}

/// Finds each literal after the end of the previous one, over a byte stream.
///
/// Leftmost matching is optimal for `%`-separated literals, so no
/// backtracking is needed. The window keeps `len - 1` bytes of the current
/// literal's prefix so matches spanning chunks are found.
fn find_literals<R: Read>(
    mut reader: R,
    finders: &[Finder<'static>],
    mut skip: usize,
) -> io::Result<bool> {
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

fn find_dfa<R: Read>(
    mut reader: R,
    dfa: &DFA,
    cache: &mut Cache,
    mut skip: usize,
) -> io::Result<bool> {
    let mut sid = dfa
        .start_state(cache, &start::Config::new().anchored(Anchored::No))
        .map_err(io::Error::other)?;
    let mut chunk = vec![0u8; STREAM_CHUNK];

    loop {
        let n = reader.read(&mut chunk)?;
        if n == 0 {
            break;
        }
        let mut data = &chunk[..n];

        if skip > 0 {
            if skip >= data.len() {
                skip -= data.len();
                continue;
            }
            data = &data[skip..];
            skip = 0;
        }

        for &b in data {
            sid = dfa.next_state(cache, sid, b).map_err(io::Error::other)?;
            if sid.is_tagged() {
                if sid.is_match() {
                    return Ok(true);
                }
                if sid.is_dead() {
                    return Ok(false);
                }
                if sid.is_quit() {
                    return Err(io::Error::other("pattern DFA quit"));
                }
            }
        }
    }

    let sid = dfa.next_eoi_state(cache, sid).map_err(io::Error::other)?;
    Ok(sid.is_match())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn matches(pattern: &str, haystack: &[u8]) -> bool {
        Matcher::compile(pattern.as_bytes())
            .unwrap()
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
        assert!(!matches("a_c", b"ac"));
        assert!(matches("_", b"z"));
        assert!(!matches("_", b""));
        assert!(matches("a_%z", b"ab----Z"));
    }

    #[test]
    fn matches_span_chunks() {
        for pattern in ["needle%tail", "ne_dle%tail"] {
            let m = Matcher::compile(pattern.as_bytes()).unwrap();
            assert!(m.find_in(Trickle(b"..NEEDLE..TAIL.."), 0).unwrap());
            assert!(!m.find_in(Trickle(b"..TAIL..NEEDLE.."), 0).unwrap());
        }
    }

    #[test]
    fn offset_skips_prefix() {
        for pattern in ["abc", "a_c"] {
            let m = Matcher::compile(pattern.as_bytes()).unwrap();
            assert!(m.find_in(&b"abc---abc"[..], 3).unwrap());
            assert!(!m.find_in(&b"abc------"[..], 1).unwrap());
        }
    }
}
