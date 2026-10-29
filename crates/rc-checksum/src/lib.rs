//! Checksums against the [`rc_vfs::Vfs`] trait: hashing files ([`hash_file`]), writing checksum
//! files ([`create`]) and checking them ([`verify`]), and splitting a file into parts with
//! Total Commander's `.crc` file ([`split`]) and putting it back together ([`combine`]).
//!
//! Total Commander is the functional reference: its extensions (`.md5`, `.sha`, `.sha256`,
//! `.sha512`, `.blake3`, `.sfv`), its `name.001`, `name.002`, ... parts and its
//! `filename=`/`size=`/`crc32=` file. The checksum lines are the `sha256sum`/`md5sum` format
//! (`<hex> *<name>`), so `sha256sum -c` checks what [`create`] writes; reading also takes the
//! BSD form (`SHA256 (name) = hex`) and SFV.
//!
//! Every file is streamed through one buffer of [`BUFFER_SIZE`], so a file of any size costs the
//! same memory. Every run takes a [`Control`]: it counts the bytes read and the files (parts,
//! lines) it plans and handles, waits at the next buffer while paused and stops there when
//! cancelled - what a job queue needs for its progress window. A run that does not finish - failed, cancelled, or its future dropped -
//! removes every file it created; files are created with `CreateNew`, so nothing that existed
//! before is ever overwritten or removed. A checksum file is untrusted input: its size is capped
//! ([`MAX_CHECKSUM_FILE`]) and a name in it that leaves its folder (absolute, `..`, a drive) is
//! reported, never opened.

mod algorithm;
mod combine;
mod control;
mod crc_file;
mod create;
mod error;
mod format;
mod names;
mod split;
mod verify;

pub use algorithm::{Algorithm, BUFFER_SIZE, Hasher, hash_file, hash_stream};
pub use combine::{CombineReport, combine};
pub use control::{Control, ReadGuard};
pub use create::{CreateReport, FileChecksum, Layout, create};
pub use error::ChecksumError;
pub use format::{ChecksumLine, MAX_CHECKSUM_FILE, parse_checksums, sfv_header, sum_line};
pub use names::plain as plain_name;
pub use split::{MAX_PARTS, SplitReport, part_name, split};
pub use verify::{VerifyEntry, VerifyReport, VerifyStatus, VerifySummary, verify};

#[cfg(test)]
mod test_support;
