//! Where the text goes: [`TextOut`] writes it into the sink in windows of at most
//! [`WRITE_CHUNK`] and stops at [`crate::Limits::max_text_bytes`]; [`Discard`] takes it when
//! only a slide's title or a sheet's size is wanted.

use crate::types::{Failure, Sink};

/// The most one `write-at` takes.
pub(crate) const WRITE_CHUNK: usize = 1 << 20;

/// Lines of text, written in order.
pub(crate) trait Lines {
    /// Appends `text`; past the cap it is dropped.
    fn push(&mut self, text: &str) -> Result<(), Failure>;
    /// Whether the cap is reached, so nothing more need be read.
    fn full(&self) -> bool;
}

/// The text of one `text` call.
pub(crate) struct TextOut<'a, K: Sink> {
    sink: &'a mut K,
    buf: String,
    written: u64,
    cap: u64,
    full: bool,
}

impl<'a, K: Sink> TextOut<'a, K> {
    pub(crate) fn new(sink: &'a mut K, cap: u64) -> Self {
        Self {
            sink,
            buf: String::new(),
            written: 0,
            cap,
            full: false,
        }
    }

    fn flush(&mut self) -> Result<(), Failure> {
        for chunk in self.buf.as_bytes().chunks(WRITE_CHUNK) {
            self.sink.write_at(self.written, chunk)?;
            self.written += chunk.len() as u64;
        }
        self.buf.clear();
        Ok(())
    }

    /// Writes what is left; the bytes written in all.
    pub(crate) fn finish(mut self) -> Result<u64, Failure> {
        self.flush()?;
        Ok(self.written)
    }
}

impl<K: Sink> Lines for TextOut<'_, K> {
    fn push(&mut self, text: &str) -> Result<(), Failure> {
        if self.full {
            return Ok(());
        }
        let used = self.written + self.buf.len() as u64;
        let room = usize::try_from(self.cap.saturating_sub(used)).unwrap_or(usize::MAX);
        if text.len() > room {
            let mut end = room;
            while !text.is_char_boundary(end) {
                end -= 1;
            }
            self.buf.push_str(text.get(..end).unwrap_or_default());
            self.full = true;
        } else {
            self.buf.push_str(text);
        }
        if self.buf.len() >= WRITE_CHUNK {
            self.flush()?;
        }
        Ok(())
    }

    fn full(&self) -> bool {
        self.full
    }
}

/// Lines nobody reads.
pub(crate) struct Discard;

impl Lines for Discard {
    fn push(&mut self, _text: &str) -> Result<(), Failure> {
        Ok(())
    }

    fn full(&self) -> bool {
        false
    }
}

#[cfg(test)]
#[path = "out_tests.rs"]
mod tests;
