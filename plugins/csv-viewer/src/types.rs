//! The viewer world's shapes, mirrored one to one from `docs/plugins.md` ("The worlds"), so the
//! guest binding (`src/guest.rs`) maps each to its WIT twin without logic of its own, and the
//! plugin's three settings. `purpose` and `theme` are not mirrored: the CSV viewer answers the
//! same for the Lister and Quick View and names token classes, never colours.

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

/// `token-class`: language-neutral, coloured by the theme. CSV gives each column one of eight in
/// turn ([`crate::tokens::COLUMN_CLASSES`]), the separators `punctuation`, a broken field
/// `invalid`.
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
    /// `info(src).name`: the file's name, never its folder.
    fn name(&self) -> String {
        String::new()
    }
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

/// The setting `separator` (`manifest.toml`, `[[settings]]`): the byte the person chose, or the
/// file's own (`auto`, [`crate::detect::separator`]).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SeparatorSetting {
    #[default]
    Auto,
    Fixed(u8),
}

impl SeparatorSetting {
    /// The setting's value; anything unknown is the default, `auto`.
    #[must_use]
    pub fn from_setting(value: Option<&str>) -> Self {
        match value {
            Some("comma") => Self::Fixed(b','),
            Some("semicolon") => Self::Fixed(b';'),
            Some("tab") => Self::Fixed(b'\t'),
            Some("pipe") => Self::Fixed(b'|'),
            _ => Self::Auto,
        }
    }
}

/// The setting `header`: whether the first record names the columns.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum HeaderSetting {
    /// When none of the first record's fields is a number ([`crate::detect::is_number`]).
    #[default]
    Auto,
    Yes,
    No,
}

impl HeaderSetting {
    /// The setting's value; anything unknown is the default, `auto`.
    #[must_use]
    pub fn from_setting(value: Option<&str>) -> Self {
        match value {
            Some("yes") => Self::Yes,
            Some("no") => Self::No,
            _ => Self::Auto,
        }
    }
}

/// The setting `encoding`: how the file's bytes are read. The text the host gets is UTF-8 either
/// way, and the tokens count its bytes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum EncodingSetting {
    /// UTF-8 when the whole file is valid UTF-8, else Windows-1252 - Excel writes CSV so on
    /// Windows.
    #[default]
    Auto,
    Utf8,
    Windows1252,
}

impl EncodingSetting {
    /// The setting's value; anything unknown is the default, `auto`.
    #[must_use]
    pub fn from_setting(value: Option<&str>) -> Self {
        match value {
            Some("utf-8") => Self::Utf8,
            Some("windows-1252") => Self::Windows1252,
            _ => Self::Auto,
        }
    }
}

/// The plugin's settings, read once when a document opens.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Settings {
    pub separator: SeparatorSetting,
    pub header: HeaderSetting,
    pub encoding: EncodingSetting,
}

impl Settings {
    /// The three settings from their stored values; a missing or unknown one is `auto`.
    #[must_use]
    pub fn from_values(
        separator: Option<&str>,
        header: Option<&str>,
        encoding: Option<&str>,
    ) -> Self {
        Self {
            separator: SeparatorSetting::from_setting(separator),
            header: HeaderSetting::from_setting(header),
            encoding: EncodingSetting::from_setting(encoding),
        }
    }
}
