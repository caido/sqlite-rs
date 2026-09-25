//! Groups plaintext bytes into characters for the NFA. A character can be
//! split across two decompressed chunks, so its leading bytes are held here
//! until it is complete.

#[derive(Default)]
pub(super) struct CharBuf {
    bytes: [u8; 4],
    len: usize,
    width: usize,
}

impl CharBuf {
    pub(super) fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Takes one byte and calls `emit` for every unit that is now ready: a
    /// whole character, or single bytes when the input is not valid UTF-8.
    pub(super) fn push(&mut self, byte: u8, mut emit: impl FnMut(&[u8])) {
        if self.len > 0 {
            if is_continuation(byte) {
                self.bytes[self.len] = byte;
                self.len += 1;
                if self.len == self.width {
                    emit(&self.bytes[..self.len]);
                    self.len = 0;
                }
                return;
            }
            self.flush(&mut emit);
        }

        match width(byte) {
            1 => emit(&[byte]),
            width => {
                self.bytes[0] = byte;
                self.len = 1;
                self.width = width;
            }
        }
    }

    /// Emits the bytes of an unfinished character one at a time.
    pub(super) fn flush(&mut self, mut emit: impl FnMut(&[u8])) {
        for &byte in &self.bytes[..self.len] {
            emit(&[byte]);
        }
        self.len = 0;
    }
}

pub(super) fn is_continuation(byte: u8) -> bool {
    byte & 0xC0 == 0x80
}

fn width(lead: u8) -> usize {
    match lead {
        0xC0..=0xDF => 2,
        0xE0..=0xEF => 3,
        0xF0..=0xF7 => 4,
        _ => 1,
    }
}
