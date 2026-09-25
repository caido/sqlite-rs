//! The pattern as an NFA over pattern positions.
//!
//! Position `i` means "the first `i` tokens are matched". The last position,
//! `tokens.len()`, means the whole pattern is matched. The state is the set of
//! positions the input seen so far can have reached. All of them are tracked
//! at once, so the matcher never goes back over input.
//!
//! One step takes one character and builds the next set:
//! - a star keeps its position, since it can absorb the character;
//! - `Any` moves forward one position;
//! - `Byte` moves forward only if the character's bytes are the next literals.
//!
//! Positions that cannot move are dropped. A star can also match nothing, so
//! after each step the position just past a live star is made live too.
//!
//! The outcome is known early in two cases: no position is left, or a trailing
//! star is reached and whatever follows cannot reject it.

use super::{pattern::Token, utf8::is_continuation};

pub(super) struct Nfa {
    tokens: Vec<Token>,
    live: Vec<bool>,
    next: Vec<bool>,
    skip: Option<u8>,
}

impl Nfa {
    pub(super) fn new(tokens: Vec<Token>) -> Self {
        let n = tokens.len();
        let mut live = vec![false; n + 1];
        live[0] = true;
        close(&tokens, &mut live);
        let mut nfa = Self {
            tokens,
            live,
            next: vec![false; n + 1],
            skip: None,
        };
        nfa.skip = nfa.find_skip();
        nfa
    }

    /// Advances every live position over one character.
    pub(super) fn step(&mut self, unit: &[u8]) {
        self.next.fill(false);
        for pos in 0..self.tokens.len() {
            if !self.live[pos] {
                continue;
            }
            match self.tokens[pos] {
                Token::Star => self.next[pos] = true,
                Token::Any => self.next[pos + 1] = true,
                Token::Byte(_) => {
                    // A multi-byte character must match as many consecutive literals.
                    if matches_literal(&self.tokens[pos..], unit) {
                        self.next[pos + unit.len()] = true;
                    }
                }
            }
        }
        close(&self.tokens, &mut self.next);
        std::mem::swap(&mut self.live, &mut self.next);
        self.skip = self.find_skip();
    }

    /// No position is left: no continuation of the input can match.
    pub(super) fn is_dead(&self) -> bool {
        !self.live.iter().any(|&live| live)
    }

    /// The whole pattern is matched by the input so far.
    pub(super) fn is_complete(&self) -> bool {
        self.live[self.tokens.len()]
    }

    /// Matched through a trailing star: any further input still matches.
    pub(super) fn is_settled(&self) -> bool {
        self.tokens.last() == Some(&Token::Star) && self.is_complete()
    }

    /// A byte the input can be skipped to without changing the state.
    pub(super) fn skip_byte(&self) -> Option<u8> {
        self.skip
    }

    /// When the only live positions are a star and the literal right after it,
    /// every other byte leads back to that same pair.
    fn find_skip(&self) -> Option<u8> {
        let mut live = self
            .live
            .iter()
            .enumerate()
            .filter(|(_, live)| **live)
            .map(|(pos, _)| pos);

        let star = live.next()?;
        let literal = live.next()?;
        if literal != star + 1 || live.next().is_some() {
            return None;
        }
        match (self.tokens[star], self.tokens.get(literal)) {
            // Jumping to a continuation byte would land mid-character.
            (Token::Star, Some(&Token::Byte(b))) if !is_continuation(b) => Some(b),
            _ => None,
        }
    }
}

/// A star matches the empty string, so a live position on a star makes the
/// next one live. Left to right, so it chains within one pass.
fn close(tokens: &[Token], set: &mut [bool]) {
    for (pos, token) in tokens.iter().enumerate() {
        if set[pos] && *token == Token::Star {
            set[pos + 1] = true;
        }
    }
}

fn matches_literal(tokens: &[Token], unit: &[u8]) -> bool {
    tokens.len() >= unit.len()
        && tokens
            .iter()
            .zip(unit)
            .all(|(token, &byte)| *token == Token::Byte(byte))
}
