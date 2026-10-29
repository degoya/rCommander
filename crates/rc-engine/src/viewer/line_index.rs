//! Line numbers for a file of any size, counted in the background and kept in bounded memory.
//!
//! [`LineIndex`] reads the file once from start to end in steps ([`LineIndex::step`], or
//! [`LineIndex::next_offset`] and [`LineIndex::feed`] for a caller that reads itself), so the
//! view can show the file before the count is done. It keeps the start of every `stride`-th
//! line only; when that list reaches [`MAX_CHECKPOINTS`] entries it drops every other one and
//! doubles the stride, so 512 KiB hold the index of any file. A line number turns into an offset
//! (and back) by reading forward from the nearest checkpoint, at most `stride` lines.

use rc_vfs::VfsError;

use super::{Detected, FileWindow, encoding::TextEncoding};

/// The most checkpoints the index keeps.
pub const MAX_CHECKPOINTS: usize = 1 << 16;

/// The stride an index starts with.
const FIRST_STRIDE: u64 = 64;

/// How much one [`LineIndex::step`] reads by default.
pub const STEP_BYTES: usize = 4 * 1024 * 1024;

const LF: u16 = 0x0A;
const CR: u16 = 0x0D;

/// Finds line starts in a file read in order, chunk by chunk.
#[derive(Debug, Clone)]
struct Scanner {
    encoding: TextEncoding,
    /// The offset of the next byte to feed.
    pos: u64,
    /// The previous unit was a CR: an LF now belongs to the same break.
    pending_cr: bool,
    /// The first byte of a UTF-16 unit cut by the chunk boundary.
    partial: Option<u8>,
}

impl Scanner {
    fn new(encoding: TextEncoding, pos: u64) -> Self {
        Self {
            encoding,
            pos,
            pending_cr: false,
            partial: None,
        }
    }

    /// Where the next chunk continues: after the units seen and a cut unit's first byte.
    fn resume(&self) -> u64 {
        self.pos + u64::from(self.partial.is_some())
    }

    /// Calls `on_start` with the offset of every line start in `chunk`; stops early when it
    /// answers `false`. Answers whether it went through the whole chunk.
    fn scan(&mut self, chunk: &[u8], mut on_start: impl FnMut(u64) -> bool) -> bool {
        let unit = self.encoding.unit();
        let mut rest = chunk;
        if let Some(first) = self.partial.take() {
            let Some((&second, tail)) = rest.split_first() else {
                self.partial = Some(first);
                return true;
            };
            if !self.unit(&[first, second], unit, &mut on_start) {
                return false;
            }
            rest = tail;
        }
        if unit == 1 {
            // Single bytes: jump from break to break instead of looking at every byte.
            let mut at = 0;
            while at < rest.len() {
                if !self.pending_cr {
                    let Some(skip) = rest[at..].iter().position(|&b| b == b'\n' || b == b'\r')
                    else {
                        self.pos += (rest.len() - at) as u64;
                        return true;
                    };
                    self.pos += skip as u64;
                    at += skip;
                }
                if !self.unit(&rest[at..=at], 1, &mut on_start) {
                    return false;
                }
                at += 1;
            }
            return true;
        }
        let mut units = rest.chunks_exact(unit);
        for bytes in &mut units {
            if !self.unit(bytes, unit, &mut on_start) {
                return false;
            }
        }
        if let [byte] = units.remainder() {
            self.partial = Some(*byte);
        }
        true
    }

    fn unit(&mut self, bytes: &[u8], unit: usize, on_start: &mut impl FnMut(u64) -> bool) -> bool {
        let at = self.pos;
        self.pos += unit as u64;
        let value = self.encoding.unit_value(bytes);
        if std::mem::take(&mut self.pending_cr) {
            if value == LF {
                return on_start(self.pos);
            }
            // A lone CR ended a line at `at`; this unit starts the next one.
            if !on_start(at) {
                return false;
            }
        }
        match value {
            LF => on_start(self.pos),
            CR => {
                self.pending_cr = true;
                true
            }
            _ => true,
        }
    }

    /// The file ended: a CR still waiting was a lone one.
    fn finish(&mut self, on_start: impl FnOnce(u64)) {
        if std::mem::take(&mut self.pending_cr) {
            on_start(self.pos);
        }
    }
}

/// The line index of one file.
#[derive(Debug, Clone)]
pub struct LineIndex {
    detected: Detected,
    scanner: Scanner,
    /// Lines between two checkpoints, a power of two.
    stride: u64,
    /// `checkpoints[i]` is where line `i * stride` starts (line 0 after the byte-order mark).
    checkpoints: Vec<u64>,
    /// Line starts seen so far, line 0 included.
    lines: u64,
    /// Where the last line seen starts.
    last_start: u64,
    complete: bool,
}

impl LineIndex {
    /// An empty index for a file in `detected`'s encoding.
    #[must_use]
    pub fn new(detected: Detected) -> Self {
        Self {
            detected,
            scanner: Scanner::new(detected.encoding, detected.bom_len),
            stride: FIRST_STRIDE,
            checkpoints: vec![detected.bom_len],
            lines: 1,
            last_start: detected.bom_len,
            complete: false,
        }
    }

    /// Where the next [`LineIndex::feed`] continues.
    #[must_use]
    pub fn next_offset(&self) -> u64 {
        self.scanner.resume()
    }

    /// Whether the whole file is counted.
    #[must_use]
    pub fn is_complete(&self) -> bool {
        self.complete
    }

    /// The lines counted so far; the file's line count once complete. A break at the very end
    /// of the file starts no further line.
    #[must_use]
    pub fn line_count(&self) -> u64 {
        if self.complete && self.lines > 1 && self.last_start == self.scanner.pos {
            self.lines - 1
        } else {
            self.lines
        }
    }

    /// The current distance between checkpoints, in lines.
    #[must_use]
    pub fn stride(&self) -> u64 {
        self.stride
    }

    /// How many checkpoints the index holds; never more than [`MAX_CHECKPOINTS`].
    #[cfg(test)]
    #[must_use]
    pub fn checkpoint_count(&self) -> usize {
        self.checkpoints.len()
    }

    /// Counts the next bytes of the file, which start at [`LineIndex::next_offset`].
    pub fn feed(&mut self, chunk: &[u8]) {
        let mut scanner = self.scanner.clone();
        scanner.scan(chunk, |start| {
            self.record(start);
            true
        });
        self.scanner = scanner;
    }

    /// The file ends at [`LineIndex::next_offset`]: the count is complete.
    pub fn finish(&mut self) {
        let mut scanner = self.scanner.clone();
        scanner.finish(|start| self.record(start));
        self.scanner = scanner;
        self.complete = true;
    }

    fn record(&mut self, start: u64) {
        if self.lines.is_multiple_of(self.stride) {
            self.checkpoints.push(start);
            if self.checkpoints.len() > MAX_CHECKPOINTS {
                let mut keep = 0;
                self.checkpoints.retain(|_| {
                    keep += 1;
                    keep % 2 == 1
                });
                self.stride *= 2;
            }
        }
        self.lines += 1;
        self.last_start = start;
    }

    /// Reads and counts up to `max_bytes` more of `window`'s file. Answers whether the index
    /// is complete.
    ///
    /// # Errors
    ///
    /// The backend failed to read.
    pub async fn step(&mut self, window: &FileWindow, max_bytes: usize) -> Result<bool, VfsError> {
        if self.complete {
            return Ok(true);
        }
        let chunk = window
            .read_uncached(self.next_offset(), max_bytes.max(1))
            .await?;
        self.feed(&chunk);
        if chunk.is_empty() || self.next_offset() >= window.size() {
            self.finish();
        }
        Ok(self.complete)
    }

    /// The nearest checkpoint at or before line `line`: its number and offset. `None` when the
    /// count has not reached `line`, or the file has no such line.
    #[must_use]
    pub fn checkpoint(&self, line: u64) -> Option<(u64, u64)> {
        if line >= self.line_count() {
            return None;
        }
        let index = usize::try_from(line / self.stride)
            .unwrap_or(usize::MAX)
            .min(self.checkpoints.len() - 1);
        Some((index as u64 * self.stride, self.checkpoints[index]))
    }

    /// Where line `line` (from 0) starts; `None` as for [`LineIndex::checkpoint`].
    ///
    /// # Errors
    ///
    /// The backend failed to read.
    #[cfg(test)]
    pub async fn line_offset(
        &self,
        window: &FileWindow,
        line: u64,
    ) -> Result<Option<u64>, VfsError> {
        let Some((from_line, from)) = self.checkpoint(line) else {
            return Ok(None);
        };
        let mut wanted = line - from_line;
        if wanted == 0 {
            return Ok(Some(from));
        }
        let mut scanner = Scanner::new(self.detected.encoding, from);
        let mut found = None;
        while found.is_none() {
            let chunk = window.read_at(scanner.resume(), super::BLOCK_SIZE).await?;
            if chunk.is_empty() {
                scanner.finish(|start| found = (wanted == 1).then_some(start));
                break;
            }
            scanner.scan(&chunk, |start| {
                wanted -= 1;
                if wanted == 0 {
                    found = Some(start);
                }
                wanted > 0
            });
        }
        Ok(found)
    }

    /// The number of the line holding the byte at `offset`; `None` when the count has not
    /// reached it.
    ///
    /// # Errors
    ///
    /// The backend failed to read.
    pub async fn line_at(&self, window: &FileWindow, offset: u64) -> Result<Option<u64>, VfsError> {
        if offset >= self.next_offset() && !self.complete {
            return Ok(None);
        }
        let index = self
            .checkpoints
            .partition_point(|&start| start <= offset)
            .saturating_sub(1);
        let mut line = index as u64 * self.stride;
        let mut scanner = Scanner::new(self.detected.encoding, self.checkpoints[index]);
        let last = self.line_count().saturating_sub(1);
        loop {
            if scanner.pos > offset || line >= last {
                return Ok(Some(line.min(last)));
            }
            let chunk = window.read_at(scanner.resume(), super::BLOCK_SIZE).await?;
            if chunk.is_empty() {
                return Ok(Some(line.min(last)));
            }
            scanner.scan(&chunk, |start| {
                if start <= offset {
                    line += 1;
                }
                start <= offset
            });
        }
    }
}

#[cfg(test)]
#[path = "line_index_tests.rs"]
mod tests;
