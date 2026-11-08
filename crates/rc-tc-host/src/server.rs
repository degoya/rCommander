//! The host's side of the protocol: one request after another from stdin, answered on stdout,
//! the plugin's work behind the [`TcPlugin`] trait so the loop is the same - and tested - on every
//! platform, while only the Windows binary has a DLL behind it.
//!
//! An extracted entry is written by the DLL into a staged file in the host's private folder,
//! streamed back as data frames under the credit window, and deleted, whatever happened. A frame
//! the loop cannot use ends it with [`ServeError`]; the binary then exits, and rCommander sees a
//! host that is gone.

use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;
use thiserror::Error;

use crate::credit::Outbound;
use crate::frame::{END, Frame, FrameError, read_frame, write_frame};
use crate::rpc::{
    CreditParams, Entry, ExtractParams, Extracted, Hello, Kind, ListParams, Listing, Message,
    PROTOCOL, Preview, PreviewParams, code, method,
};

/// The bytes of one data frame the host sends.
pub const CHUNK: usize = 256 * 1024;
/// The largest preview side the host hands on, in pixels.
pub const MAX_PREVIEW: u32 = 4096;

/// Why the plugin could not do what was asked.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum PluginError {
    /// The DLL lacks the kind, or this platform loads no DLL.
    #[error("{0}")]
    Unsupported(String),
    /// The DLL refused or failed the call.
    #[error("{0}")]
    Failed(String),
}

/// A preview as the plugin made it: rows of BGRA, top row first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bitmap {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// `width * height * 4` bytes.
    pub bgra: Vec<u8>,
}

/// What a loaded TC DLL does for the host.
pub trait TcPlugin {
    /// The kinds the DLL exports.
    fn kinds(&self) -> Vec<Kind>;
    /// The entries of `archive`.
    fn list(&mut self, archive: &Path) -> Result<Vec<Entry>, PluginError>;
    /// Writes the entry named `entry` of `archive` to the file `target`.
    fn extract(&mut self, archive: &Path, entry: &str, target: &Path) -> Result<(), PluginError>;
    /// A preview of `file` within `width` x `height`.
    fn preview(&mut self, file: &Path, width: u32, height: u32) -> Result<Bitmap, PluginError>;
}

/// Why the loop ended early.
#[derive(Debug, Error)]
pub enum ServeError {
    /// Reading a frame failed.
    #[error(transparent)]
    Frame(#[from] FrameError),
    /// Writing to rCommander failed.
    #[error("writing an answer failed: {0}")]
    Write(#[from] io::Error),
    /// rCommander sent what the protocol does not allow.
    #[error("protocol violation: {0}")]
    Protocol(String),
}

/// An answer that went wrong before it was sent.
enum Refusal {
    Answer(i64, String),
    Fatal(ServeError),
}

impl From<ServeError> for Refusal {
    fn from(error: ServeError) -> Self {
        Self::Fatal(error)
    }
}

impl From<PluginError> for Refusal {
    fn from(error: PluginError) -> Self {
        match error {
            PluginError::Unsupported(message) => Self::Answer(code::UNSUPPORTED, message),
            PluginError::Failed(message) => Self::Answer(code::PLUGIN, message),
        }
    }
}

/// The loop over one plugin.
pub struct Server<P> {
    plugin: P,
    staging: PathBuf,
    staged: u64,
}

impl<P: TcPlugin> Server<P> {
    /// A loop for `plugin`, staging extracted entries in `staging` (a folder only this host uses).
    pub fn new(plugin: P, staging: PathBuf) -> Self {
        Self {
            plugin,
            staging,
            staged: 0,
        }
    }

    /// Answers requests until rCommander closes the pipe (`Ok`) or breaks the protocol.
    pub fn serve(
        &mut self,
        reader: &mut impl Read,
        writer: &mut impl Write,
    ) -> Result<(), ServeError> {
        loop {
            let json = match read_frame(reader) {
                Ok(Frame::Control(json)) => json,
                Ok(Frame::Data { .. }) => {
                    return Err(ServeError::Protocol("a data frame to the host".to_owned()));
                }
                Err(FrameError::Closed) => return Ok(()),
                Err(error) => return Err(error.into()),
            };
            let message = Message::from_json(&json)
                .ok_or_else(|| ServeError::Protocol("not a JSON-RPC 2.0 message".to_owned()))?;
            let (Some(id), Some(name)) = (message.id, message.method.as_deref()) else {
                if message.method.is_some() {
                    // A late `$/credit` for a stream that has ended.
                    continue;
                }
                return Err(ServeError::Protocol(
                    "an answer sent to the host".to_owned(),
                ));
            };
            let params = message.params.unwrap_or(Value::Null);
            let answer = match self.handle(name, params, reader, writer) {
                Ok(result) => Message::answer(id, result),
                Err(Refusal::Answer(code, text)) => Message::failure(id, code, text),
                Err(Refusal::Fatal(error)) => return Err(error),
            };
            write_frame(writer, &Frame::Control(answer.to_json()))?;
        }
    }

    fn handle(
        &mut self,
        name: &str,
        params: Value,
        reader: &mut impl Read,
        writer: &mut impl Write,
    ) -> Result<Value, Refusal> {
        match name {
            method::HELLO => to_value(&Hello {
                protocol: PROTOCOL,
                arch: std::env::consts::ARCH.to_owned(),
                kinds: self.plugin.kinds(),
            }),
            method::WCX_LIST => {
                let params: ListParams = from_value(params)?;
                let entries = self.plugin.list(absolute(&params.archive)?)?;
                to_value(&Listing { entries })
            }
            method::WCX_EXTRACT => {
                let params: ExtractParams = from_value(params)?;
                let archive = absolute(&params.archive)?;
                self.staged += 1;
                let target = self.staging.join(format!("{}.part", self.staged));
                let sent = self.extract(archive, &params, &target, reader, writer);
                // The staged copy goes whatever happened; one that was never written is no error.
                let _ = fs::remove_file(&target);
                to_value(&Extracted { size: sent? })
            }
            method::WLX_PREVIEW => {
                let params: PreviewParams = from_value(params)?;
                let file = absolute(&params.file)?;
                let (width, height) = (
                    params.width.clamp(1, MAX_PREVIEW),
                    params.height.clamp(1, MAX_PREVIEW),
                );
                let bitmap = self.plugin.preview(file, width, height)?;
                check_bitmap(&bitmap, width, height)?;
                send_stream(params.stream, &mut bitmap.bgra.as_slice(), reader, writer)?;
                to_value(&Preview {
                    width: bitmap.width,
                    height: bitmap.height,
                })
            }
            other => Err(Refusal::Answer(
                code::METHOD_NOT_FOUND,
                format!("unknown method {other}"),
            )),
        }
    }

    fn extract(
        &mut self,
        archive: &Path,
        params: &ExtractParams,
        target: &Path,
        reader: &mut impl Read,
        writer: &mut impl Write,
    ) -> Result<u64, Refusal> {
        self.plugin.extract(archive, &params.entry, target)?;
        let mut staged = File::open(target).map_err(host_io)?;
        send_stream(params.stream, &mut staged, reader, writer)
    }
}

/// Sends `source` as data frames on `stream`, waiting for `$/credit` whenever the window is
/// spent, and closes it with an empty [`END`] frame; the bytes sent.
fn send_stream(
    stream: u32,
    source: &mut impl Read,
    reader: &mut impl Read,
    writer: &mut impl Write,
) -> Result<u64, Refusal> {
    let mut credit = Outbound::default();
    let mut buffer = vec![0_u8; CHUNK];
    let mut sent = 0_u64;
    loop {
        let read = source.read(&mut buffer).map_err(host_io)?;
        if read == 0 {
            break;
        }
        while !credit.spend(read as u64) {
            wait_for_credit(stream, &mut credit, reader)?;
        }
        let frame = Frame::Data {
            stream,
            flags: 0,
            bytes: buffer[..read].to_vec(),
        };
        write_frame(writer, &frame).map_err(ServeError::Write)?;
        sent += read as u64;
    }
    let end = Frame::Data {
        stream,
        flags: END,
        bytes: Vec::new(),
    };
    write_frame(writer, &end).map_err(ServeError::Write)?;
    Ok(sent)
}

/// Reads frames until a `$/credit` for `stream` arrives; credit for an ended stream is passed
/// over, anything else while a stream is open breaks the protocol.
fn wait_for_credit(
    stream: u32,
    credit: &mut Outbound,
    reader: &mut impl Read,
) -> Result<(), ServeError> {
    loop {
        let Frame::Control(json) = read_frame(reader)? else {
            return Err(ServeError::Protocol("a data frame to the host".to_owned()));
        };
        let message = Message::from_json(&json)
            .filter(|message| {
                message.id.is_none() && message.method.as_deref() == Some(method::CREDIT)
            })
            .ok_or_else(|| ServeError::Protocol("a request while a stream is open".to_owned()))?;
        let params: CreditParams = message
            .params
            .and_then(|params| serde_json::from_value(params).ok())
            .ok_or_else(|| ServeError::Protocol("a $/credit without its params".to_owned()))?;
        if params.stream == stream {
            return credit
                .grant(u64::from(params.bytes))
                .map_err(|error| ServeError::Protocol(error.to_string()));
        }
    }
}

fn check_bitmap(bitmap: &Bitmap, width: u32, height: u32) -> Result<(), Refusal> {
    let expected = u64::from(bitmap.width) * u64::from(bitmap.height) * 4;
    if bitmap.width > width || bitmap.height > height || bitmap.bgra.len() as u64 != expected {
        return Err(Refusal::Answer(
            code::PLUGIN,
            format!(
                "the plugin's preview of {}x{} does not fit {width}x{height}",
                bitmap.width, bitmap.height
            ),
        ));
    }
    Ok(())
}

fn absolute(path: &str) -> Result<&Path, Refusal> {
    let path = Path::new(path);
    if path.is_absolute() {
        Ok(path)
    } else {
        Err(Refusal::Answer(
            code::INVALID_PARAMS,
            "the path is not absolute".to_owned(),
        ))
    }
}

fn host_io(error: io::Error) -> Refusal {
    Refusal::Answer(code::HOST_IO, format!("the staged file: {error}"))
}

fn from_value<T: DeserializeOwned>(params: Value) -> Result<T, Refusal> {
    serde_json::from_value(params)
        .map_err(|error| Refusal::Answer(code::INVALID_PARAMS, error.to_string()))
}

fn to_value(value: &impl Serialize) -> Result<Value, Refusal> {
    Ok(serde_json::to_value(value).expect("the host's answers are plain JSON structs"))
}

#[cfg(test)]
#[path = "server_tests.rs"]
mod tests;
