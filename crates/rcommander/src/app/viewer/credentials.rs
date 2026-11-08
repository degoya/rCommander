//! Credential files coloured by the core itself (RC-110-07). The read guard
//! (`rc_mcp::sensitive_path`) keeps credential files from every viewer plugin (decision 8,
//! `docs/plugins.md`): `.env`, `.npmrc`, `winscp.ini`, `rclone.conf`, `.cargo/credentials.toml`,
//! `logins.json`, `.docker/config.json`, `gh/hosts.yml`, FileZilla's site files and their kin. The
//! bundled plugins that claim their formats never see them, so the built-in viewer shows them,
//! and this module adds the colours, as `exif.rs` adds a photo's EXIF: a document with the one
//! view `highlighted` over the Lister's own file.
//!
//! The colours are the bundled viewers' own: their crates are plain Rust libraries, linked here
//! natively - the config viewer for `.env`, INI and TOML, the JSON, YAML and XML viewers - each
//! run in process over the file's bytes in memory, only for the tokens of the file's own lines
//! (no pretty layout, no tree). No plugin runtime, no session file: nothing is written anywhere.
//! A file is coloured when the guard names it, its name gives one of these formats, the Lister
//! shows it as UTF-8 text and it is at most [`MAX_BYTES`]; it is read once when it opens and only
//! the tokens are kept. `plugins.viewer_token_colors` switches the colours off, as for a plugin.

use std::{collections::HashMap, path::Path, sync::Arc};

use rc_engine::viewer::{TextEncoding, ViewMode};

use super::{
    document::{Answer, DocView, Document, DocumentSource, Token, TokenClass, TreeNode},
    load::Opened,
};

/// The largest credential file coloured; a larger one shows as before.
pub const MAX_BYTES: u64 = 1 << 20;
/// The lines asked of a viewer at once.
const CHUNK_LINES: u32 = 4096;
/// The extensions the config viewer reads that credential files carry.
const CONFIG_EXTENSIONS: &[&str] = &["cfg", "conf", "env", "ini", "npmrc", "properties", "toml"];

/// The bundled viewer whose tokens colour a credential file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    /// `.env`, `.env.*`, INI, TOML, properties: `plugins/config-viewer`.
    Config,
    Json,
    Yaml,
    Xml,
}

/// The format of a file named `name`, by its extension in any case; `.env` and `.env.*` are the
/// config viewer's unless the last extension names JSON, YAML or XML.
pub fn format_of(name: &str) -> Option<Format> {
    let lower = name.to_ascii_lowercase();
    match lower.rsplit_once('.').map(|(_, extension)| extension) {
        Some("json") => Some(Format::Json),
        Some("yml" | "yaml") => Some(Format::Yaml),
        Some("xml") => Some(Format::Xml),
        Some(extension) if CONFIG_EXTENSIONS.contains(&extension) => Some(Format::Config),
        _ if lower == ".env" || lower.starts_with(".env.") => Some(Format::Config),
        _ => None,
    }
}

/// The tokens of a credential file the Lister opened at `path`; `None` when the read guard does
/// not name it, its name gives no format, it is not shown as UTF-8 text, it is larger than
/// [`MAX_BYTES`], or it cannot be read.
pub async fn read(path: &Path, opened: &Opened) -> Option<Arc<ColoredLines>> {
    rc_mcp::sensitive_path(path)?;
    let name = path.file_name()?.to_str()?;
    let format = format_of(name)?;
    if opened.mode != ViewMode::Text || opened.detected.encoding != TextEncoding::Utf8 {
        return None;
    }
    let size = opened.file.size();
    if size > MAX_BYTES {
        return None;
    }
    let bytes = opened
        .file
        .read_uncached(0, usize::try_from(size).ok()?)
        .await
        .ok()?;
    let start = usize::try_from(opened.detected.bom_len).ok()?;
    let starts = line_starts(&bytes, start);
    let file = MemoryFile {
        name: name.to_owned(),
        bytes,
    };
    let lines = tokens(format, file)?;
    Some(Arc::new(ColoredLines::new(&starts, lines)))
}

/// The document of a credential file: the coloured text, the Lister's own file.
pub fn document(lines: Arc<ColoredLines>) -> Option<Document> {
    let source: Arc<dyn DocumentSource> = Arc::new(ColoredSource { lines });
    Document::new(vec![DocView::Highlighted], source)
}

/// The tokens of each line of a file, by the byte offset where the line starts.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct ColoredLines {
    lines: HashMap<u64, Vec<Token>>,
}

impl ColoredLines {
    /// The token lists of the lines starting at `starts`, in their order; a line without tokens
    /// is not kept, lists past the last start are dropped.
    pub fn new(starts: &[u64], lists: Vec<Vec<Token>>) -> Self {
        let lines = starts
            .iter()
            .zip(lists)
            .filter(|(_, tokens)| !tokens.is_empty())
            .map(|(start, tokens)| (*start, tokens))
            .collect();
        Self { lines }
    }

    /// The tokens of the line starting at `start`; none for a line without colour.
    pub fn line(&self, start: u64) -> Vec<Token> {
        self.lines.get(&start).cloned().unwrap_or_default()
    }
}

/// Where each line of `bytes` starts, from `start` (after a byte-order mark): a line ends at
/// `\n`, as the Lister and the viewers split them.
pub fn line_starts(bytes: &[u8], start: usize) -> Vec<u64> {
    let mut starts = Vec::new();
    let mut at = start;
    while at < bytes.len() {
        starts.push(at as u64);
        at = bytes[at..]
            .iter()
            .position(|byte| *byte == b'\n')
            .map_or(bytes.len(), |found| at + found + 1);
    }
    starts
}

/// The document's source: the tokens taken when the file opened.
#[derive(Debug)]
struct ColoredSource {
    lines: Arc<ColoredLines>,
}

impl DocumentSource for ColoredSource {
    fn tokens(&self, starts: Vec<u64>) -> Answer<Vec<Vec<Token>>> {
        let lines: Vec<Vec<Token>> = starts.iter().map(|start| self.lines.line(*start)).collect();
        Box::pin(async move { Ok(lines) })
    }

    // The document has no tree; nothing asks.
    fn children(&self, _node: Option<u64>) -> Answer<Vec<TreeNode>> {
        Box::pin(async { Ok(Vec::new()) })
    }
}

/// A file in memory, handed to a bundled viewer as its source.
struct MemoryFile {
    name: String,
    bytes: Vec<u8>,
}

impl MemoryFile {
    /// Up to `length` bytes at `offset`; none past the end.
    fn window(&self, offset: u64, length: u32) -> Vec<u8> {
        let size = self.bytes.len();
        let start = usize::try_from(offset).map_or(size, |offset| offset.min(size));
        let end = start.saturating_add(length as usize).min(size);
        self.bytes[start..end].to_vec()
    }
}

impl rc_plugin_config_viewer::types::Source for MemoryFile {
    fn size(&self) -> u64 {
        self.bytes.len() as u64
    }

    fn name(&self) -> String {
        self.name.clone()
    }

    fn read_at(
        &mut self,
        offset: u64,
        length: u32,
    ) -> Result<Vec<u8>, rc_plugin_config_viewer::types::Failure> {
        Ok(self.window(offset, length))
    }
}

impl rc_plugin_json_viewer::types::Source for MemoryFile {
    fn size(&self) -> u64 {
        self.bytes.len() as u64
    }

    fn name(&self) -> String {
        self.name.clone()
    }

    fn read_at(
        &mut self,
        offset: u64,
        length: u32,
    ) -> Result<Vec<u8>, rc_plugin_json_viewer::types::Failure> {
        Ok(self.window(offset, length))
    }
}

impl rc_plugin_yaml_viewer::types::Source for MemoryFile {
    fn size(&self) -> u64 {
        self.bytes.len() as u64
    }

    fn read_at(
        &mut self,
        offset: u64,
        length: u32,
    ) -> Result<Vec<u8>, rc_plugin_yaml_viewer::types::Failure> {
        Ok(self.window(offset, length))
    }
}

impl rc_plugin_xml_viewer::types::Source for MemoryFile {
    fn size(&self) -> u64 {
        self.bytes.len() as u64
    }

    fn read_at(
        &mut self,
        offset: u64,
        length: u32,
    ) -> Result<Vec<u8>, rc_plugin_xml_viewer::types::Failure> {
        Ok(self.window(offset, length))
    }
}

/// Every line's tokens from the open viewer `$viewer` of the crate `$krate`, as the window's
/// tokens; `None` (from the enclosing function) when the viewer fails.
macro_rules! all_tokens {
    ($krate:ident, $viewer:expr) => {{
        use $krate::types::TokenClass as Theirs;
        let mut viewer = $viewer;
        let total = viewer.line_count();
        let mut lines: Vec<Vec<Token>> = Vec::new();
        while (lines.len() as u64) < total {
            let first = u32::try_from(lines.len()).ok()?;
            let chunk = viewer.tokens(first, CHUNK_LINES).ok()?;
            if chunk.is_empty() {
                break;
            }
            lines.extend(chunk.into_iter().map(|line| {
                line.into_iter()
                    .map(|token| Token {
                        start: token.start,
                        len: token.len,
                        class: match token.class {
                            Theirs::Keyword => TokenClass::Keyword,
                            Theirs::Key => TokenClass::Key,
                            Theirs::String => TokenClass::String,
                            Theirs::Number => TokenClass::Number,
                            Theirs::Literal => TokenClass::Literal,
                            Theirs::Comment => TokenClass::Comment,
                            Theirs::Punctuation => TokenClass::Punctuation,
                            Theirs::Operator => TokenClass::Operator,
                            Theirs::TypeName => TokenClass::TypeName,
                            Theirs::Function => TokenClass::Function,
                            Theirs::Invalid => TokenClass::Invalid,
                        },
                    })
                    .collect()
            }));
        }
        Some(lines)
    }};
}

/// The tokens of every line of `file` as the bundled viewer of `format` colours them: the file's
/// own lines, in the encoding the viewer detects (UTF-8 here, a byte-order mark skipped).
fn tokens(format: Format, file: MemoryFile) -> Option<Vec<Vec<Token>>> {
    match format {
        Format::Config => all_tokens!(
            rc_plugin_config_viewer,
            rc_plugin_config_viewer::Viewer::open(file, None).ok()?.0
        ),
        Format::Json => all_tokens!(
            rc_plugin_json_viewer,
            rc_plugin_json_viewer::Viewer::open(file, rc_plugin_json_viewer::types::Format::AsIs)
                .ok()?
                .0
        ),
        Format::Yaml => all_tokens!(
            rc_plugin_yaml_viewer,
            rc_plugin_yaml_viewer::Viewer::open(file).ok()?.0
        ),
        Format::Xml => all_tokens!(
            rc_plugin_xml_viewer,
            rc_plugin_xml_viewer::Viewer::open(file, rc_plugin_xml_viewer::types::Format::AsIs)
                .ok()?
                .0
        ),
    }
}

#[cfg(test)]
#[path = "credentials_tests.rs"]
mod tests;
