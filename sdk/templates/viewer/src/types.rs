//! The viewer world's shapes this plugin uses, mirrored from `wit/rcommander.wit`, so the logic
//! in `lib.rs` and `table.rs` is plain Rust that `cargo test` runs without a WebAssembly
//! toolchain. `src/guest.rs` maps each to its WIT twin and nothing more.

/// `output`: what a document can be shown as. This viewer names `markdown` or `text`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Output {
    Text,
    Markdown,
}

/// `document`: the outputs a document offers, the first shown.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Document {
    pub outputs: Vec<Output>,
    pub title: Option<String>,
}

/// `failure-kind`, the cases this viewer answers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FailureKind {
    BadData,
    Cancelled,
    Io,
}

/// `failure`: a kind and an optional code from `locales/en.json`, which starts with the plugin's
/// id.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Failure {
    pub kind: FailureKind,
    pub code: Option<String>,
}

impl Failure {
    /// A failure of `kind` without a code of its own.
    #[must_use]
    pub fn of(kind: FailureKind) -> Self {
        Self { kind, code: None }
    }
}

/// The `source` interface over one handle: the file being viewed, read in windows.
pub trait Source {
    /// `info(src).name`: the file name, never its folder.
    fn name(&self) -> String;
    /// `info(src).size`.
    fn size(&self) -> u64;
    /// `read-at`: at most `length` bytes (at most 1 MiB) from `offset`; fewer only at the end.
    fn read_at(&mut self, offset: u64, length: u32) -> Result<Vec<u8>, Failure>;
    /// `should-stop`: the person moved on; the running call ends with `cancelled`.
    fn should_stop(&mut self) -> bool {
        false
    }
}

/// The `sink` interface over one handle: where `text` writes.
pub trait Sink {
    /// `write-at`: at most 1 MiB at `offset`.
    fn write_at(&mut self, offset: u64, bytes: &[u8]) -> Result<(), Failure>;
}
