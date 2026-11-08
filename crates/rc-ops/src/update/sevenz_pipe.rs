//! A 7z block encoded again without the entries it lost.
//!
//! The decoder pushes (`BlockDecoder::for_each_entries` hands each entry's data to a callback),
//! the encoder pulls (`push_archive_entries` reads one source after the other), so the block is
//! decoded on a thread of its own into a bounded channel the encoder's sources read from: neither
//! the block nor an entry is ever held whole in memory. An encoder that fails drops the channel
//! and so stops the decoder; a decoder that fails ends the channel early, which fails the
//! encoder - the decoder's error is the one reported.

use std::{
    cell::RefCell,
    io::{self, Read, Seek, Write},
    rc::Rc,
    sync::mpsc,
};

use sevenz_rust2::{
    Archive, ArchiveEntry, ArchiveWriter, BlockDecoder, Error as SevenError, Password, SourceReader,
};

use crate::pack::{
    sevenz::seven_error,
    write::{BUFFER, panicked},
};
use crate::recover::LogPanic as _;

/// One message of the channel: a piece of an entry's data, or its end.
enum Piece {
    Data(Vec<u8>),
    End,
}

/// One block of the old archive to encode again with only the entries `kept` (indices into the
/// archive's files, ascending, each with data in `block`).
pub(super) struct Again<'a> {
    pub(super) archive: &'a Archive,
    pub(super) block: usize,
    pub(super) kept: &'a [usize],
}

impl Again<'_> {
    /// Decodes the block out of `source` and pushes the kept entries into `writer` as one block.
    pub(super) fn encode<R, W>(
        &self,
        source: &mut R,
        writer: &mut ArchiveWriter<W>,
    ) -> io::Result<()>
    where
        R: Read + Seek + Send,
        W: Write + Seek,
    {
        let (tx, rx) = mpsc::sync_channel::<Piece>(4);
        let inbox = Rc::new(RefCell::new(Inbox {
            rx,
            chunk: Vec::new(),
            at: 0,
        }));
        let entries: Vec<ArchiveEntry> = self
            .kept
            .iter()
            .filter_map(|&index| self.archive.files.get(index).cloned())
            .collect();
        let readers: Vec<SourceReader<Piped>> = entries
            .iter()
            .map(|_| {
                SourceReader::new(Piped {
                    inbox: Rc::clone(&inbox),
                    done: false,
                })
            })
            .collect();
        let key = Password::empty();
        std::thread::scope(|scope| {
            let decoding = scope.spawn(|| self.decode(source, &key, tx));
            let pushed = writer
                .push_archive_entries(entries, readers)
                .map(|_| ())
                .map_err(seven_error);
            // The receiver goes with the last reader: a decoder still sending stops.
            drop(inbox);
            let decoded = match decoding.join().log_panic("ops.archive_update_decoder") {
                Ok(decoded) => decoded,
                Err(_) => Err(panicked()),
            };
            match (decoded, pushed) {
                (Err(error), _) | (Ok(_), Err(error)) => Err(error),
                (Ok(true), Ok(())) => Ok(()),
                (Ok(false), Ok(())) => Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "the block was not encoded again whole",
                )),
            }
        })
    }

    /// Sends the kept entries' data, each followed by its end, and lets the others go to
    /// nowhere; `false` when the encoder stopped reading.
    fn decode<R: Read + Seek>(
        &self,
        source: &mut R,
        key: &Password,
        tx: mpsc::SyncSender<Piece>,
    ) -> io::Result<bool> {
        let mut at = self
            .archive
            .stream_map
            .block_first_file_index
            .get(self.block)
            .copied()
            .unwrap_or(0);
        let mut stopped = false;
        let decoder = BlockDecoder::new(1, self.block, self.archive, key, source);
        let walked = decoder.for_each_entries(&mut |_: &ArchiveEntry, data: &mut dyn Read| {
            let here = at;
            at += 1;
            if self.kept.binary_search(&here).is_err() {
                // An entry removed, or one without data: decoded to nowhere.
                io::copy(data, &mut io::sink()).map_err(SevenError::from)?;
                return Ok(true);
            }
            let mut buffer = vec![0_u8; BUFFER];
            loop {
                let read = data.read(&mut buffer).map_err(SevenError::from)?;
                if read == 0 {
                    break;
                }
                if tx.send(Piece::Data(buffer[..read].to_vec())).is_err() {
                    stopped = true;
                    return Ok(false);
                }
            }
            if tx.send(Piece::End).is_err() {
                stopped = true;
                return Ok(false);
            }
            Ok(true)
        });
        match walked {
            Ok(_) => Ok(!stopped),
            Err(error) => Err(seven_error(error)),
        }
    }
}

/// The channel's receiving end, shared by the sources of one block, which read one after the
/// other.
struct Inbox {
    rx: mpsc::Receiver<Piece>,
    chunk: Vec<u8>,
    at: usize,
}

/// One kept entry's data, as the encoder reads it.
struct Piped {
    inbox: Rc<RefCell<Inbox>>,
    done: bool,
}

impl Read for Piped {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.done || buf.is_empty() {
            return Ok(0);
        }
        let mut inbox = self.inbox.borrow_mut();
        while inbox.at >= inbox.chunk.len() {
            match inbox.rx.recv() {
                Ok(Piece::Data(chunk)) => {
                    inbox.chunk = chunk;
                    inbox.at = 0;
                }
                Ok(Piece::End) => {
                    self.done = true;
                    return Ok(0);
                }
                Err(_) => {
                    return Err(io::Error::new(
                        io::ErrorKind::UnexpectedEof,
                        "the block being encoded again ended early",
                    ));
                }
            }
        }
        let from = inbox.at;
        let count = buf.len().min(inbox.chunk.len() - from);
        buf[..count].copy_from_slice(&inbox.chunk[from..from + count]);
        inbox.at += count;
        Ok(count)
    }
}
