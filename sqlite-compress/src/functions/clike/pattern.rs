//! Turns a `LIKE` pattern into the tokens the NFA walks.

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Token {
    /// A byte that must appear as-is.
    Byte(u8),
    /// `_`: exactly one character.
    Any,
    /// `%`: any run of characters, including none.
    Star,
}

pub(super) fn compile(pattern: &[u8]) -> Vec<Token> {
    let mut tokens = Vec::with_capacity(pattern.len());
    for &byte in pattern {
        let token = match byte {
            b'%' => Token::Star,
            b'_' => Token::Any,
            byte => Token::Byte(byte),
        };
        // `%%` matches what `%` matches. One star keeps the skip fast path reachable.
        if token == Token::Star && tokens.last() == Some(&Token::Star) {
            continue;
        }
        tokens.push(token);
    }
    tokens
}
