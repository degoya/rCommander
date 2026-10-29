//! The viewer world's shapes, mirrored one to one from `docs/plugins.md` ("The worlds"), so the
//! guest binding (`src/guest.rs`, wave 3 with RC-100-12) maps each to its WIT twin without logic
//! of its own. `purpose` and `theme` are not mirrored: the JSON viewer answers the same for the
//! Lister and Quick View and names token classes, never colours.

/// `output`: what a document can be shown as.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Output {
    Text,
    Highlighted,
    Markdown,
    Image,
    Tree,
    Pages,
}

/// `document`: the outputs a document offers, the first shown.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Document {
    pub outputs: Vec<Output>,
    pub title: Option<String>,
    pub pages: Option<u32>,
}

/// `tree-node`: one node of the `tree` output; children are asked for by `id`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TreeNode {
    pub id: u64,
    pub label: String,
    pub value: Option<String>,
    pub has_children: bool,
}

/// `token-class`: language-neutral, coloured by the theme. JSON uses `key`, `string`, `number`,
/// `literal`, `punctuation` and `invalid`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TokenClass {
    Keyword,
    Key,
    String,
    Number,
    Literal,
    Comment,
    Punctuation,
    Operator,
    TypeName,
    Function,
    Invalid,
}

/// `token`: a span of UTF-8 bytes within its line.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Token {
    pub start: u32,
    pub len: u32,
    pub class: TokenClass,
}

/// `failure-kind`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FailureKind {
    NotFound,
    Exists,
    AccessDenied,
    NotSupported,
    BadData,
    WrongPassword,
    Cancelled,
    Io,
    Other,
}

/// `failure`: a kind, an optional code under the plugin's slug (`locales/en.json`) and its
/// parameters.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Failure {
    pub kind: FailureKind,
    pub code: Option<String>,
    pub params: Vec<(String, String)>,
}

impl Failure {
    /// A failure of `kind` without a code of its own.
    #[must_use]
    pub fn of(kind: FailureKind) -> Self {
        Self {
            kind,
            code: None,
            params: Vec::new(),
        }
    }

    /// A failure of `kind` with one of the codes in `locales/en.json`.
    #[must_use]
    pub fn coded(kind: FailureKind, code: &str) -> Self {
        Self {
            kind,
            code: Some(code.to_owned()),
            params: Vec::new(),
        }
    }
}

/// The `source` interface over one handle: the file being viewed, read in windows.
pub trait Source {
    /// `info(src).size`.
    fn size(&self) -> u64;
    /// `read-at`: at most `length` bytes (at most 1 MiB) from `offset`; fewer only at the end.
    fn read_at(&mut self, offset: u64, length: u32) -> Result<Vec<u8>, Failure>;
    /// `should-stop`: the person moved on; the running call ends with `cancelled`.
    fn should_stop(&mut self) -> bool {
        false
    }
    /// `progress`.
    fn progress(&mut self, _done: u64, _total: Option<u64>) {}
}

/// The `sink` interface over one handle: where `text` writes.
pub trait Sink {
    /// `write-at`: at most 1 MiB at `offset`.
    fn write_at(&mut self, offset: u64, bytes: &[u8]) -> Result<(), Failure>;
}

/// The setting `format` (`manifest.toml`, `[[settings]]`): the text as the file has it, or
/// indented, since a minified file is otherwise one line.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Format {
    #[default]
    AsIs,
    Pretty,
}

impl Format {
    /// The setting's value; anything but `pretty` is the default, `as_is`.
    #[must_use]
    pub fn from_setting(value: Option<&str>) -> Self {
        match value {
            Some("pretty") => Self::Pretty,
            _ => Self::AsIs,
        }
    }
}
