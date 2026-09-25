//! Streaming entry point: plaintext bytes in, verdict out.
//!
//! Bytes are grouped into whole characters, each character is one NFA step,
//! and the verdict is checked after every step so the scan can stop early.
//! While the NFA is only waiting inside a star for one literal byte, input is
//! skipped straight to that byte with `memchr`.

use memchr::memchr;

use super::{nfa::Nfa, pattern, utf8::CharBuf};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Verdict {
    Match,
    NoMatch,
    NeedMore,
}

pub(super) struct LikeMatcher {
    nfa: Nfa,
    chars: CharBuf,
}

impl LikeMatcher {
    pub(super) fn new(pattern: &[u8]) -> Self {
        Self {
            nfa: Nfa::new(pattern::compile(pattern)),
            chars: CharBuf::default(),
        }
    }

    /// Answer so far, from what has been pushed.
    pub(super) fn verdict(&self) -> Verdict {
        if self.nfa.is_settled() {
            Verdict::Match
        } else if self.nfa.is_dead() {
            Verdict::NoMatch
        } else {
            Verdict::NeedMore
        }
    }

    /// Feeds the next plaintext chunk. Returns as soon as the answer is known.
    pub(super) fn push(&mut self, mut chunk: &[u8]) -> Verdict {
        while !chunk.is_empty() {
            if let Some(target) = self.skip_target() {
                match memchr(target, chunk) {
                    Some(i) => chunk = &chunk[i..],
                    None => return Verdict::NeedMore,
                }
            }

            self.chars.push(chunk[0], |unit| self.nfa.step(unit));
            chunk = &chunk[1..];

            match self.verdict() {
                Verdict::NeedMore => {}
                verdict => return verdict,
            }
        }
        Verdict::NeedMore
    }

    /// End of the value: whether the whole pattern was matched.
    pub(super) fn finish(&mut self) -> bool {
        self.chars.flush(|unit| self.nfa.step(unit));
        self.nfa.is_complete()
    }

    fn skip_target(&self) -> Option<u8> {
        if self.chars.is_empty() {
            self.nfa.skip_byte()
        } else {
            None
        }
    }
}
