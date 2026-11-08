//! rCommander's side of the protocol: requests to the host, one at a time, and the checks of
//! what comes back - the host runs a foreign DLL, so its answers are outside data like a remote
//! server's.
//!
//! The pipe ending is [`BridgeError::HostGone`]: the DLL crashed the host, or it was ended. A
//! frame the protocol does not allow is [`BridgeError::Protocol`]; the owner of the process
//! ([`HostProcess`](crate::process::HostProcess)) then kills the host. Either way the bridge is
//! spent, and a new host is started for the next call.

use std::io::{self, Read, Write};
use std::path::Path;

use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use thiserror::Error;

use crate::credit::Inbound;
use crate::frame::{Frame, FrameError, read_frame, write_frame};
use crate::rpc::{
    CreditParams, Entry, Extracted, Hello, Listing, Message, PROTOCOL, Preview, method,
};
use crate::server::{Bitmap, MAX_PREVIEW};
use crate::wcx::safe_entry_name;

/// Why a call to the host failed.
#[derive(Debug, Error)]
pub enum BridgeError {
    /// The host ended - a crash in the DLL, or it was ended.
    #[error("the plugin host ended")]
    HostGone,
    /// The host sent what the protocol does not allow.
    #[error("the plugin host broke the protocol: {0}")]
    Protocol(String),
    /// Reading from or writing to the pipe failed.
    #[error("talking to the plugin host failed: {0}")]
    Io(#[source] io::Error),
    /// The host answered with an error.
    #[error("{message}")]
    Remote {
        /// One of [`crate::rpc::code`].
        code: i64,
        /// The host's message.
        message: String,
    },
    /// Writing the received bytes where the caller wanted them failed.
    #[error("writing the plugin's data failed: {0}")]
    Sink(#[source] io::Error),
    /// A path the protocol cannot carry (not valid Unicode).
    #[error("the path is not valid Unicode")]
    Path,
    /// The DLL cannot run in this host (its machine, or no DLL at all).
    #[error("{0}")]
    Refused(String),
    /// Starting the host failed.
    #[error("starting the plugin host failed: {0}")]
    Start(#[source] io::Error),
}

/// An archive's listing as rCommander takes it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Checked {
    /// The entries whose names stay below an extraction folder.
    pub entries: Vec<Entry>,
    /// The entries left out for their names (`..`, absolute, a drive, a stream).
    pub rejected: usize,
}

/// The protocol over a reader and a writer - the host's stdout and stdin, or pipes in a test.
pub struct Bridge<R, W> {
    reader: R,
    writer: W,
    next_id: u64,
    next_stream: u32,
    spent: bool,
}

impl<R: Read, W: Write> Bridge<R, W> {
    /// A bridge over the host's `reader` (its stdout) and `writer` (its stdin).
    pub fn new(reader: R, writer: W) -> Self {
        Self {
            reader,
            writer,
            next_id: 1,
            next_stream: 1,
            spent: false,
        }
    }

    /// Whether the host ended or broke the protocol; every later call is [`BridgeError::HostGone`].
    pub fn is_spent(&self) -> bool {
        self.spent
    }

    /// The host's protocol, architecture and the DLL's kinds; a protocol other than this
    /// crate's is refused.
    pub fn hello(&mut self) -> Result<Hello, BridgeError> {
        let hello: Hello = typed(self.call(method::HELLO, Value::Null, None)?)?;
        if hello.protocol != PROTOCOL {
            return Err(
                self.violation(format!("protocol {} instead of {PROTOCOL}", hello.protocol))
            );
        }
        Ok(hello)
    }

    /// The entries of `archive`, the unsafe names left out and counted.
    pub fn list(&mut self, archive: &Path) -> Result<Checked, BridgeError> {
        let archive = text(archive)?;
        let params = json!({ "archive": archive });
        let listing: Listing = typed(self.call(method::WCX_LIST, params, None)?)?;
        let total = listing.entries.len();
        let entries: Vec<Entry> = listing
            .entries
            .into_iter()
            .filter(|entry| safe_entry_name(&entry.name))
            .collect();
        Ok(Checked {
            rejected: total - entries.len(),
            entries,
        })
    }

    /// Writes the entry `entry` of `archive` into `sink`; the bytes written.
    pub fn extract(
        &mut self,
        archive: &Path,
        entry: &str,
        sink: &mut dyn Write,
    ) -> Result<u64, BridgeError> {
        let stream = self.stream();
        let archive = text(archive)?;
        let params = json!({ "archive": archive, "entry": entry, "stream": stream });
        let mut counted = Counted { sink, bytes: 0 };
        let target: &mut dyn Write = &mut counted;
        let answer: Extracted =
            typed(self.call(method::WCX_EXTRACT, params, Some((stream, target)))?)?;
        if answer.size != counted.bytes {
            return Err(self.violation(format!(
                "{} bytes announced, {} received",
                answer.size, counted.bytes
            )));
        }
        Ok(counted.bytes)
    }

    /// A preview of `file` within `width` x `height` (each at most [`MAX_PREVIEW`]).
    pub fn preview(&mut self, file: &Path, width: u32, height: u32) -> Result<Bitmap, BridgeError> {
        let (width, height) = (width.clamp(1, MAX_PREVIEW), height.clamp(1, MAX_PREVIEW));
        let stream = self.stream();
        let file = text(file)?;
        let params = json!({ "file": file, "width": width, "height": height, "stream": stream });
        let limit = u64::from(width) * u64::from(height) * 4;
        let mut pixels = Limited {
            bytes: Vec::new(),
            limit,
        };
        let target: &mut dyn Write = &mut pixels;
        let answer: Preview =
            typed(self.call(method::WLX_PREVIEW, params, Some((stream, target)))?)?;
        let expected = u64::from(answer.width) * u64::from(answer.height) * 4;
        if answer.width > width || answer.height > height || pixels.bytes.len() as u64 != expected {
            return Err(self.violation(format!(
                "a preview of {}x{} with {} bytes",
                answer.width,
                answer.height,
                pixels.bytes.len()
            )));
        }
        Ok(Bitmap {
            width: answer.width,
            height: answer.height,
            bgra: pixels.bytes,
        })
    }

    fn stream(&mut self) -> u32 {
        let stream = self.next_stream;
        self.next_stream = self.next_stream.wrapping_add(1).max(1);
        stream
    }

    fn violation(&mut self, what: String) -> BridgeError {
        self.spent = true;
        BridgeError::Protocol(what)
    }

    /// Sends one request and reads until its answer, writing the data frames of `sink`'s stream
    /// into it and returning their credit. A failing sink does not stop the reading: the stream
    /// is drained so the pipe stays in step, and the sink's error is returned at the answer.
    fn call(
        &mut self,
        name: &str,
        params: Value,
        mut sink: Option<(u32, &mut dyn Write)>,
    ) -> Result<Value, BridgeError> {
        if self.spent {
            return Err(BridgeError::HostGone);
        }
        let id = self.next_id;
        self.next_id += 1;
        let request = Frame::Control(Message::request(id, name, params).to_json());
        if let Err(error) = write_frame(&mut self.writer, &request) {
            self.spent = true;
            return Err(gone_or(error));
        }
        let mut inbound = Inbound::default();
        let mut sink_error = None;
        loop {
            let frame = match read_frame(&mut self.reader) {
                Ok(frame) => frame,
                Err(FrameError::Closed) => {
                    self.spent = true;
                    return Err(BridgeError::HostGone);
                }
                Err(FrameError::Io(error)) => {
                    self.spent = true;
                    return Err(gone_or(error));
                }
                Err(error) => return Err(self.violation(error.to_string())),
            };
            match frame {
                Frame::Data { stream, bytes, .. } => {
                    let Some((_, target)) = sink.as_mut().filter(|(open, _)| *open == stream)
                    else {
                        return Err(self.violation(format!("data on stream {stream}")));
                    };
                    if let Err(error) = inbound.receive(bytes.len() as u64) {
                        return Err(self.violation(error.to_string()));
                    }
                    if sink_error.is_none()
                        && let Err(error) = target.write_all(&bytes)
                    {
                        sink_error = Some(error);
                    }
                    inbound.release(bytes.len() as u64);
                    if !bytes.is_empty() {
                        self.credit(stream, bytes.len())?;
                    }
                }
                Frame::Control(json) => {
                    let message = Message::from_json(&json)
                        .filter(|message| message.id == Some(id) && message.method.is_none())
                        .ok_or_else(|| self.violation("an answer to no request".to_owned()))?;
                    if let Some(error) = sink_error {
                        return Err(BridgeError::Sink(error));
                    }
                    return match (message.result, message.error) {
                        (_, Some(error)) => Err(BridgeError::Remote {
                            code: error.code,
                            message: error.message,
                        }),
                        (Some(result), None) => Ok(result),
                        (None, None) => {
                            Err(self.violation("an answer without a result".to_owned()))
                        }
                    };
                }
            }
        }
    }

    fn credit(&mut self, stream: u32, bytes: usize) -> Result<(), BridgeError> {
        let bytes =
            u32::try_from(bytes).map_err(|_| self.violation("a frame over 4 GiB".to_owned()))?;
        let params = serde_json::to_value(CreditParams { stream, bytes })
            .expect("two integers are plain JSON");
        let note = Frame::Control(Message::notification(method::CREDIT, params).to_json());
        write_frame(&mut self.writer, &note).map_err(|error| {
            self.spent = true;
            gone_or(error)
        })
    }
}

/// A sink that counts what passed.
struct Counted<'a> {
    sink: &'a mut dyn Write,
    bytes: u64,
}

impl Write for Counted<'_> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let written = self.sink.write(buf)?;
        self.bytes += written as u64;
        Ok(written)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.sink.flush()
    }
}

/// A buffer that refuses more than `limit` bytes, so a host cannot grow a preview without end.
struct Limited {
    bytes: Vec<u8>,
    limit: u64,
}

impl Write for Limited {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if self.bytes.len() as u64 + buf.len() as u64 > self.limit {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "a preview over its size",
            ));
        }
        self.bytes.extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// A broken pipe is a host that ended; anything else an I/O error.
fn gone_or(error: io::Error) -> BridgeError {
    match error.kind() {
        io::ErrorKind::BrokenPipe | io::ErrorKind::UnexpectedEof => BridgeError::HostGone,
        _ => BridgeError::Io(error),
    }
}

fn text(path: &Path) -> Result<&str, BridgeError> {
    path.to_str().ok_or(BridgeError::Path)
}

fn typed<T: DeserializeOwned>(value: Value) -> Result<T, BridgeError> {
    serde_json::from_value(value)
        .map_err(|error| BridgeError::Protocol(format!("an answer of the wrong shape: {error}")))
}

#[cfg(test)]
#[path = "bridge_tests.rs"]
mod tests;
