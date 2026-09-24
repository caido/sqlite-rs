use memchr::memchr;
use sqlite_ffi::SqliteError;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Token {
    Byte(u8),
    Any,
    Star,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Verdict {
    Match,
    NoMatch,
    NeedMore,
}

pub struct LikeMatcher {
    tokens: Vec<Token>,
    trailing_star: bool,
    active: Vec<bool>,
    next: Vec<bool>,
    skip: Option<u8>,
    pending: [u8; 4],
    pending_len: usize,
    pending_width: usize,
}

impl LikeMatcher {
    pub fn new(pattern: &[u8], escape: Option<u8>) -> Result<Self, SqliteError> {
        let mut tokens = Vec::with_capacity(pattern.len());
        let mut bytes = pattern.iter().copied();
        while let Some(b) = bytes.next() {
            let token = if Some(b) == escape {
                Token::Byte(bytes.next().ok_or_else(|| {
                    SqliteError::Message("escape character must be followed by a character".into())
                })?)
            } else {
                match b {
                    b'%' => Token::Star,
                    b'_' => Token::Any,
                    b => Token::Byte(b),
                }
            };
            if token == Token::Star && tokens.last() == Some(&Token::Star) {
                continue;
            }
            tokens.push(token);
        }

        let n = tokens.len();
        let mut matcher = Self {
            trailing_star: tokens.last() == Some(&Token::Star),
            tokens,
            active: vec![false; n + 1],
            next: vec![false; n + 1],
            skip: None,
            pending: [0; 4],
            pending_len: 0,
            pending_width: 0,
        };
        matcher.active[0] = true;
        close(&mut matcher.active, &matcher.tokens);
        matcher.skip = matcher.compute_skip();
        Ok(matcher)
    }

    pub fn verdict(&self) -> Verdict {
        if self.trailing_star && self.active[self.tokens.len()] {
            Verdict::Match
        } else if self.active.iter().any(|&a| a) {
            Verdict::NeedMore
        } else {
            Verdict::NoMatch
        }
    }

    pub fn push(&mut self, mut chunk: &[u8]) -> Verdict {
        while let Some((&b, rest)) = chunk.split_first() {
            if self.pending_len == 0 {
                if let Some(target) = self.skip {
                    if b != target {
                        match memchr(target, chunk) {
                            Some(i) => chunk = &chunk[i..],
                            None => return Verdict::NeedMore,
                        }
                        continue;
                    }
                }
            }

            chunk = rest;
            self.push_byte(b);
            match self.verdict() {
                Verdict::NeedMore => {}
                verdict => return verdict,
            }
        }
        Verdict::NeedMore
    }

    pub fn finish(&mut self) -> bool {
        self.flush_pending();
        self.active[self.tokens.len()]
    }

    fn push_byte(&mut self, b: u8) {
        if self.pending_len > 0 {
            if b & 0xC0 == 0x80 {
                self.pending[self.pending_len] = b;
                self.pending_len += 1;
                if self.pending_len == self.pending_width {
                    let unit = self.pending;
                    let len = self.pending_len;
                    self.pending_len = 0;
                    self.step(&unit[..len]);
                }
                return;
            }
            self.flush_pending();
        }

        let width = match b {
            0xC0..=0xDF => 2,
            0xE0..=0xEF => 3,
            0xF0..=0xF7 => 4,
            _ => 1,
        };
        if width == 1 {
            self.step(&[b]);
        } else {
            self.pending[0] = b;
            self.pending_len = 1;
            self.pending_width = width;
        }
    }

    fn flush_pending(&mut self) {
        let unit = self.pending;
        let len = self.pending_len;
        self.pending_len = 0;
        for &b in &unit[..len] {
            self.step(&[b]);
        }
    }

    fn step(&mut self, unit: &[u8]) {
        let n = self.tokens.len();
        self.next.fill(false);

        for s in 0..n {
            if !self.active[s] {
                continue;
            }
            match self.tokens[s] {
                Token::Star => self.next[s] = true,
                Token::Any => self.next[s + 1] = true,
                Token::Byte(_) => {
                    let end = s + unit.len();
                    if end <= n
                        && self.tokens[s..end]
                            .iter()
                            .zip(unit)
                            .all(|(t, &b)| *t == Token::Byte(b))
                    {
                        self.next[end] = true;
                    }
                }
            }
        }

        close(&mut self.next, &self.tokens);
        std::mem::swap(&mut self.active, &mut self.next);
        self.skip = self.compute_skip();
    }

    fn compute_skip(&self) -> Option<u8> {
        let mut on = self
            .active
            .iter()
            .enumerate()
            .filter(|(_, &a)| a)
            .map(|(i, _)| i);
        let s = on.next()?;
        if on.next() != Some(s + 1) || on.next().is_some() {
            return None;
        }
        match (self.tokens[s], self.tokens.get(s + 1)) {
            // A continuation byte can't be a jump target: we'd land mid-character.
            (Token::Star, Some(&Token::Byte(b))) if b & 0xC0 != 0x80 => Some(b),
            _ => None,
        }
    }
}

fn close(set: &mut [bool], tokens: &[Token]) {
    for (i, token) in tokens.iter().enumerate() {
        if set[i] && *token == Token::Star {
            set[i + 1] = true;
        }
    }
}
