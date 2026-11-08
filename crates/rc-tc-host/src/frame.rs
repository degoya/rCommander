//! The wire format between rCommander and `rc-tc-host`: length-prefixed frames on the host's
//! stdin and stdout.
//!
//! A frame is a `u32` length (little endian) of what follows, a type byte, then the body:
//!
//! - type [`CONTROL`]: one JSON-RPC 2.0 message as UTF-8 JSON;
//! - type [`DATA`]: a `u32` stream id, a flags byte ([`END`] on the last frame of a stream) and
//!   the raw bytes, which pass no codec.
//!
//! A frame longer than [`MAX_FRAME`], an unknown type byte or a data frame too short for its
//! header is a protocol violation: the reader stops, and the side that read it ends the other.

use std::io::{self, Read, Write};

use thiserror::Error;

/// The type byte of a JSON-RPC control message.
pub const CONTROL: u8 = 0;
/// The type byte of a data frame on a stream.
pub const DATA: u8 = 1;
/// The flag of a stream's last data frame.
pub const END: u8 = 1;
/// The largest frame either side accepts, type byte and body: a listing of a large archive is one
/// control message, data frames stay within the credit window anyway.
pub const MAX_FRAME: u32 = 16 * 1024 * 1024;
/// The bytes of a data frame's header in front of its payload: the stream id and the flags.
const DATA_HEADER: usize = 5;

/// One frame as read from or written to the pipe.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Frame {
    /// A JSON-RPC message, still as its JSON bytes.
    Control(Vec<u8>),
    /// Raw bytes on a stream.
    Data {
        /// The stream the request named.
        stream: u32,
        /// [`END`] or nothing.
        flags: u8,
        /// The payload.
        bytes: Vec<u8>,
    },
}

/// Why a frame could not be read.
#[derive(Debug, Error)]
pub enum FrameError {
    /// The other side closed its end before a frame began.
    #[error("the pipe was closed")]
    Closed,
    /// Reading failed or the pipe ended inside a frame.
    #[error("reading a frame failed: {0}")]
    Io(#[from] io::Error),
    /// The length prefix is zero or above [`MAX_FRAME`].
    #[error("a frame of {0} bytes is outside the limit")]
    Length(u32),
    /// The type byte is neither [`CONTROL`] nor [`DATA`].
    #[error("unknown frame type {0}")]
    Type(u8),
    /// A data frame without room for its stream id and flags.
    #[error("a data frame too short for its header")]
    ShortData,
}

/// Reads one frame; [`FrameError::Closed`] when the pipe ends cleanly before it.
pub fn read_frame(reader: &mut impl Read) -> Result<Frame, FrameError> {
    let mut prefix = [0_u8; 4];
    if let Err(error) = reader.read_exact(&mut prefix) {
        return Err(if error.kind() == io::ErrorKind::UnexpectedEof {
            FrameError::Closed
        } else {
            FrameError::Io(error)
        });
    }
    let length = u32::from_le_bytes(prefix);
    if length == 0 || length > MAX_FRAME {
        return Err(FrameError::Length(length));
    }
    let mut kind = [0_u8; 1];
    reader.read_exact(&mut kind)?;
    let mut body = vec![0_u8; length as usize - 1];
    reader.read_exact(&mut body)?;
    match kind[0] {
        CONTROL => Ok(Frame::Control(body)),
        DATA => {
            if body.len() < DATA_HEADER {
                return Err(FrameError::ShortData);
            }
            let stream = u32::from_le_bytes([body[0], body[1], body[2], body[3]]);
            let flags = body[4];
            Ok(Frame::Data {
                stream,
                flags,
                bytes: body.split_off(DATA_HEADER),
            })
        }
        other => Err(FrameError::Type(other)),
    }
}

/// Writes one frame and flushes it; a frame over [`MAX_FRAME`] is refused before a byte is sent.
pub fn write_frame(writer: &mut impl Write, frame: &Frame) -> io::Result<()> {
    let (kind, header, body): (u8, Vec<u8>, &[u8]) = match frame {
        Frame::Control(json) => (CONTROL, Vec::new(), json.as_slice()),
        Frame::Data {
            stream,
            flags,
            bytes,
        } => {
            let mut header = stream.to_le_bytes().to_vec();
            header.push(*flags);
            (DATA, header, bytes.as_slice())
        }
    };
    let length = 1 + header.len() + body.len();
    let length = u32::try_from(length)
        .ok()
        .filter(|length| *length <= MAX_FRAME)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "frame over the limit"))?;
    writer.write_all(&length.to_le_bytes())?;
    writer.write_all(&[kind])?;
    writer.write_all(&header)?;
    writer.write_all(body)?;
    writer.flush()
}

#[cfg(test)]
#[path = "frame_tests.rs"]
mod tests;
