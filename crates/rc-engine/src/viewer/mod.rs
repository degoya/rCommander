//! The viewer's model, without a window: what F3 (Lister) and Quick View show of a file of any
//! size, over the `Vfs` trait so it works the same on a local disk, in an archive or on a server.
//!
//! - [`FileWindow`] reads windows of a file through a bounded block cache; nothing ever loads a
//!   whole file.
//! - [`encoding`] detects the text encoding (byte-order mark, UTF-16 shape, UTF-8 validity,
//!   else Windows-1252) and decodes a window that may cut a character at either end; [`cp437`]
//!   is DOS's code page, and which files open in it (`viewer.dos_extensions`).
//! - [`text`] reads pages of lines from a byte offset and walks back for scrolling up;
//!   [`LineIndex`] counts lines in the background in bounded memory.
//! - [`hex`] makes the hex view's rows, as `xxd` prints them; [`search`] finds text or bytes
//!   forward or backward with progress and cancellation; [`sniff`] tells an image from text.
//! - [`read`] and [`secret`] serve `files.read_text` and `files.read_hex`, the MCP side: capped
//!   windows, private keys masked.
//!
//! The window that draws all this is the GUI's (`crates/rcommander`); decoding images too.

pub mod cp437;
pub mod encoding;
pub mod hex;
mod line_index;
pub mod read;
pub mod search;
pub mod secret;
pub mod sniff;
mod source;
pub mod text;

#[cfg(test)]
pub(crate) mod test_support;

pub use cp437::{dos_extensions, is_dos_file};
pub use encoding::{
    DETECT_BYTES, Decoded, Detected, TextEncoding, decode_text, decode_window, detect, detect_dos,
    detect_whole,
};
pub use hex::{BYTES_PER_ROW, HexRow};
pub use line_index::{LineIndex, MAX_CHECKPOINTS, STEP_BYTES};
pub use read::{HexWindow, ReadHexRequest, ReadTextRequest, TextWindow};
pub use search::{Direction, Match, Needle, PatternError, SearchOutcome, search};
pub use sniff::{ImageFormat, ViewMode};
pub use source::{BLOCK_SIZE, CACHE_BLOCKS, FileWindow};
pub use text::{LineEnding, TextLine, TextPage};
