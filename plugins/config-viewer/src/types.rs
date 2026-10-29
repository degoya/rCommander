//! The viewer world's shapes, mirrored one to one from `docs/plugins.md` ("The worlds"), so the
//! guest binding (`src/guest.rs`) maps each to its WIT twin without logic of its own. `purpose`
//! and `theme` are not mirrored: the viewer answers the same for the Lister and Quick View and
//! names token classes, never colours. Beside them the file's [`Format`] and the setting
//! `encoding`.

use rc_plugin_text::Encoding;

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

/// `token-class`: language-neutral, coloured by the theme. A configuration file uses every class
/// but `function`.
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

/// The setting `encoding` (`manifest.toml`, `[[settings]]`): `None` for `auto`, which `open`
/// resolves from the file, else the encoding it names. A value the manifest does not offer is
/// `auto`.
#[must_use]
pub fn encoding_setting(value: Option<&str>) -> Option<Encoding> {
    value.and_then(|value| value.parse().ok())
}

/// The syntax a file is read in, chosen by its name.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Format {
    /// `.ini`, `.inf`, `.cfg`, `.conf`, `.editorconfig`, `.gitconfig`: `[section]`, `key = value`,
    /// `;` and `#` comments.
    #[default]
    Ini,
    /// A Windows registry export, `.reg`: `[HKEY_...]` keys, `"name"=` and `@=` values typed
    /// `dword:` or `hex(...):`.
    Registry,
    /// `.env`: `KEY=value`, `export`, quoted values over lines.
    Env,
    /// Java `.properties`: `key=value`, `key: value` or `key value`, `\` continuing a value.
    Properties,
    /// TOML, `Cargo.lock` too.
    Toml,
    /// `.gitignore` and `.gitattributes`: a pattern a line, attributes after it.
    Lines,
}

impl Format {
    /// The format of a file named `name`; INI for any name not otherwise known.
    #[must_use]
    pub fn from_name(name: &str) -> Self {
        let name = name.to_ascii_lowercase();
        if name == "cargo.lock" {
            return Self::Toml;
        }
        if name.starts_with(".env.") {
            return Self::Env;
        }
        match name.rsplit_once('.').map(|(_, extension)| extension) {
            Some("reg") => Self::Registry,
            Some("env") => Self::Env,
            Some("properties") => Self::Properties,
            Some("toml") => Self::Toml,
            Some("gitignore" | "gitattributes") => Self::Lines,
            _ => Self::Ini,
        }
    }
}
