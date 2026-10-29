//! An OpenSSH client configuration (`~/.ssh/config`) read into plain connection candidates for
//! the import (RC-050-06).
//!
//! Deliberately simple, and the import dialog says so:
//!
//! - Every literal name on a `Host` line is one candidate; a name with a wildcard (`*`, `?`) or
//!   a negation (`!`) is skipped with its reason, and its options are no defaults for others.
//! - Of the options only `HostName`, `User`, `Port`, `IdentityFile` and `ProxyJump` are taken;
//!   every other keyword is listed on the candidate as not imported. The first value wins, as in
//!   OpenSSH, also across several `Host` blocks naming the same alias (in any case).
//! - `Match` blocks are skipped up to the next `Host` or `Match` line.
//! - `Include` reads its files (relative to the `.ssh` directory, globs allowed) in place, with
//!   cycles and a nesting deeper than [`MAX_INCLUDE_DEPTH`] reported, not followed; under MCP
//!   only files inside `.ssh` that the read guard lets through (`scope`). No file is read past
//!   [`MAX_FILE_BYTES`], each file at most once, and at most [`MAX_INCLUDES`] includes in all
//!   (`include`).
//! - At most [`MAX_HOSTS`] hosts and as many pattern names are taken, each host with at most
//!   [`MAX_IDENTITY_FILES`] key paths and [`MAX_IGNORED_OPTIONS`] ignored keywords; a line that
//!   can change nothing any more is not applied again (`options`), so the work grows with the
//!   text, not with hosts times lines.
//! - `IdentityFile` stays a path (`~` and `%d`, `%u`, `%h`, `%n`, `%r`, `%p`, `%k`, `%j`
//!   expanded); no key file is ever opened. Values of options that are not imported are never
//!   kept, so nothing the file holds besides the five options reaches the result.
//! - A file named like a key (`id_*`, `*.pem`, `*.key`, `*.ppk`, `*.pub`, `authorized_keys`,
//!   `authorized_keys2`, `known_hosts`) is never opened, and a file holding key text
//!   (`-----BEGIN`, `PuTTY-User-Key-File`) stops the whole import ([`LoadError::KeyMaterial`],
//!   `guard`).

mod expand;
mod guard;
mod include;
mod lex;
mod options;
mod scope;
mod types;

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use expand::{expand_tilde, expand_tokens, has_wildcard};
pub use guard::{LoadError, holds_key, is_key_file};
pub use scope::MAX_FILE_BYTES;
pub(crate) use scope::McpScope;
use scope::read_text;
pub use types::{
    ImportWarning, ImportedHost, SkipReason, SkippedHost, SourceLine, SshConfigContext,
    SshConfigImport, WarningKind,
};

/// How deep `Include` may nest below the file read first.
pub const MAX_INCLUDE_DEPTH: usize = 8;

/// How many files `Include` may name in all, however they nest; the rest are skipped.
pub const MAX_INCLUDES: usize = 256;

/// How many hosts one import takes, and how many pattern names it lists.
pub const MAX_HOSTS: usize = 1024;

/// How many `IdentityFile` paths a host keeps: OpenSSH's own limit.
pub const MAX_IDENTITY_FILES: usize = 100;

/// How many ignored keywords a host lists.
pub const MAX_IGNORED_OPTIONS: usize = 64;

/// Reads the configuration at `path` and the files it includes. Blocking file I/O.
///
/// # Errors
///
/// [`LoadError::Io`] when `path` itself cannot be read (a problem in an included file is a
/// warning instead); [`LoadError::KeyMaterial`] when `path` or an included file is a key.
pub fn load(path: &Path, context: &SshConfigContext) -> Result<SshConfigImport, LoadError> {
    load_scoped(path, context, None)
}

/// [`load`], its includes held to `scope` when an MCP call imports.
pub(crate) fn load_scoped(
    path: &Path,
    context: &SshConfigContext,
    scope: Option<McpScope>,
) -> Result<SshConfigImport, LoadError> {
    let canonical = fs::canonicalize(path)?;
    if is_key_file(path) || is_key_file(&canonical) {
        return Err(LoadError::KeyMaterial {
            file: path.to_path_buf(),
        });
    }
    let text = read_text(&canonical)?;
    let mut reader = Reader::new(context);
    reader.scope = scope;
    reader.read_files.insert(canonical.clone());
    reader.reading.push(canonical);
    reader.read_root(path, &text)
}

/// Reads configuration text as if it were the file `origin`; its includes come from disk. The
/// tests' way in: the import reads a file ([`load`]).
///
/// # Errors
///
/// [`LoadError::KeyMaterial`] when the text or an included file is a key.
#[cfg(test)]
pub(crate) fn parse_str(
    text: &str,
    origin: &Path,
    context: &SshConfigContext,
) -> Result<SshConfigImport, LoadError> {
    Reader::new(context).read_root(origin, text)
}

/// Which hosts the lines being read apply to.
#[derive(Debug, Clone)]
enum Block {
    /// Before the first `Host`: every host, which the import does not apply.
    Global,
    /// Under a `Host` line: the candidates it names literally (none for a pattern-only line).
    Hosts(options::HostBlock),
    /// Under a `Match` line: skipped.
    Match,
}

/// A value with the line it came from, expanded once the host is complete.
#[derive(Debug)]
struct Located {
    value: String,
    source: SourceLine,
}

/// A candidate while the files are read.
#[derive(Debug)]
struct Draft {
    alias: String,
    host_name: Option<Located>,
    user: Option<String>,
    port: Option<u16>,
    identity_files: Vec<Located>,
    /// `Some(None)` for `ProxyJump none`, which also takes the first-value slot.
    proxy_jump: Option<Option<String>>,
    ignored_options: Vec<String>,
    source: SourceLine,
}

struct Reader<'a> {
    context: &'a SshConfigContext,
    hosts: Vec<Draft>,
    /// The index in `hosts` of each alias, in ASCII lower case.
    aliases: HashMap<String, usize>,
    skipped: Vec<SkippedHost>,
    /// The names in `skipped`.
    skipped_names: HashSet<String>,
    warnings: Vec<ImportWarning>,
    /// The canonical paths of the files being read, outermost first, for cycle detection.
    reading: Vec<PathBuf>,
    /// The canonical paths of every file read so far: none is read twice.
    read_files: HashSet<PathBuf>,
    /// The files `Include` named so far, up to [`MAX_INCLUDES`].
    includes: usize,
    /// [`WarningKind::TooManyIncludes`] or [`WarningKind::TooManyHosts`] was given: once each.
    includes_capped: bool,
    hosts_capped: bool,
    /// The first file found to be key material; reading stops there.
    key_material: Option<PathBuf>,
    /// Where includes may lead under MCP; anywhere when `None`.
    scope: Option<McpScope>,
}

impl<'a> Reader<'a> {
    fn new(context: &'a SshConfigContext) -> Self {
        Self {
            context,
            hosts: Vec::new(),
            aliases: HashMap::new(),
            skipped: Vec::new(),
            skipped_names: HashSet::new(),
            warnings: Vec::new(),
            reading: Vec::new(),
            read_files: HashSet::new(),
            includes: 0,
            includes_capped: false,
            hosts_capped: false,
            key_material: None,
            scope: None,
        }
    }

    /// Reads the outermost file and finishes, unless it or an include is key material.
    fn read_root(mut self, file: &Path, text: &str) -> Result<SshConfigImport, LoadError> {
        if holds_key(text) {
            self.key_material = Some(file.to_path_buf());
        } else {
            self.read(file, text, Block::Global, 0);
        }
        match self.key_material.take() {
            Some(file) => Err(LoadError::KeyMaterial { file }),
            None => Ok(self.finish()),
        }
    }

    fn warn(&mut self, source: SourceLine, kind: WarningKind) {
        self.warnings.push(ImportWarning { source, kind });
    }

    /// Reads `text` from `file`, starting under `block`. The caller's block is unchanged
    /// afterwards: after an `Include` the including block goes on, as in OpenSSH.
    fn read(&mut self, file: &Path, text: &str, mut block: Block, depth: usize) {
        let text = text.strip_prefix('\u{feff}').unwrap_or(text);
        for (index, line) in text.lines().enumerate() {
            if self.key_material.is_some() {
                return;
            }
            let source = SourceLine {
                file: file.to_path_buf(),
                line: index + 1,
            };
            let directive = match lex::split_line(line) {
                Ok(Some(directive)) => directive,
                Ok(None) => continue,
                Err(error) => {
                    self.warn(source, error.into());
                    continue;
                }
            };
            match directive.keyword.to_ascii_lowercase().as_str() {
                "host" => block = self.host_line(&directive.args, &source),
                "match" => {
                    self.warn(source, WarningKind::MatchSkipped);
                    block = Block::Match;
                }
                "include" => self.include(&directive.args, &block, &source, depth),
                _ => self.option(&mut block, directive, source),
            }
        }
    }

    fn host_line(&mut self, names: &[String], source: &SourceLine) -> Block {
        if names.is_empty() {
            let keyword = "Host".to_owned();
            self.warn(source.clone(), WarningKind::MissingArgument { keyword });
        }
        let mut targets = Vec::new();
        let mut named = HashSet::new();
        for name in names {
            let reason = if name.starts_with('!') {
                Some(SkipReason::Negated)
            } else if has_wildcard(name) {
                Some(SkipReason::Pattern)
            } else {
                None
            };
            match reason {
                Some(reason) => self.skip(name, reason, source),
                None => {
                    if let Some(index) = self.candidate(name, source)
                        && named.insert(index)
                    {
                        targets.push(index);
                    }
                }
            }
        }
        Block::Hosts(options::HostBlock::new(targets))
    }

    /// Lists `name` as skipped, once, while fewer than [`MAX_HOSTS`] are.
    fn skip(&mut self, name: &str, reason: SkipReason, source: &SourceLine) {
        if self.skipped_names.contains(name) {
            return;
        }
        if self.skipped.len() >= MAX_HOSTS {
            return self.too_many_hosts(source);
        }
        self.skipped_names.insert(name.to_owned());
        self.skipped.push(SkippedHost {
            name: name.to_owned(),
            reason,
            source: source.clone(),
        });
    }

    /// The index of the candidate `alias`, added when it is new; `None` for a new one past
    /// [`MAX_HOSTS`].
    fn candidate(&mut self, alias: &str, source: &SourceLine) -> Option<usize> {
        let key = alias.to_ascii_lowercase();
        if let Some(&index) = self.aliases.get(&key) {
            return Some(index);
        }
        if self.hosts.len() >= MAX_HOSTS {
            self.too_many_hosts(source);
            return None;
        }
        self.aliases.insert(key, self.hosts.len());
        self.hosts.push(Draft {
            alias: alias.to_owned(),
            host_name: None,
            user: None,
            port: None,
            identity_files: Vec::new(),
            proxy_jump: None,
            ignored_options: Vec::new(),
            source: source.clone(),
        });
        Some(self.hosts.len() - 1)
    }

    /// Warns once that names past [`MAX_HOSTS`] are not taken.
    fn too_many_hosts(&mut self, source: &SourceLine) {
        if !self.hosts_capped {
            self.hosts_capped = true;
            self.warn(source.clone(), WarningKind::TooManyHosts);
        }
    }

    fn finish(mut self) -> SshConfigImport {
        let drafts = std::mem::take(&mut self.hosts);
        let hosts = drafts
            .into_iter()
            .map(|draft| self.finish_host(draft))
            .collect();
        SshConfigImport {
            hosts,
            skipped: self.skipped,
            warnings: self.warnings,
        }
    }

    /// Expands the candidate's `HostName` and `IdentityFile` values now that all are known.
    fn finish_host(&mut self, draft: Draft) -> ImportedHost {
        let host_name = draft.host_name.map(|located| {
            let tokens = [('h', draft.alias.as_str())];
            self.expand(&located.value, &located.source, &tokens)
        });
        let proxy_jump = draft.proxy_jump.flatten();
        let port = draft.port.unwrap_or(22).to_string();
        let home = self.context.home.to_string_lossy().into_owned();
        let local_user = self.context.local_user.clone();
        let tokens = [
            ('d', home.as_str()),
            ('u', local_user.as_str()),
            ('h', host_name.as_deref().unwrap_or(&draft.alias)),
            ('n', draft.alias.as_str()),
            ('r', draft.user.as_deref().unwrap_or(&local_user)),
            ('p', port.as_str()),
            ('k', draft.alias.as_str()),
            ('j', proxy_jump.as_deref().unwrap_or("")),
        ];
        let identity_files = draft
            .identity_files
            .iter()
            .map(|located| {
                let value = expand_tilde(&located.value, &self.context.home);
                PathBuf::from(self.expand(&value, &located.source, &tokens))
            })
            .collect();
        ImportedHost {
            alias: draft.alias,
            host_name,
            user: draft.user,
            port: draft.port,
            identity_files,
            proxy_jump,
            ignored_options: draft.ignored_options,
            source: draft.source,
        }
    }

    /// `value` with `tokens` expanded; each unknown token is a warning about `source`.
    fn expand(&mut self, value: &str, source: &SourceLine, tokens: &[(char, &str)]) -> String {
        let mut unknown = Vec::new();
        let value = expand_tokens(value, tokens, &mut unknown);
        for token in unknown {
            self.warn(source.clone(), WarningKind::UnknownToken { token });
        }
        value
    }
}

#[cfg(test)]
#[path = "ssh_config_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "ssh_config_limits_tests.rs"]
mod limits_tests;
