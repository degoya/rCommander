//! From a parsed ssh_config to the connection tree (RC-050-06): the candidates the repository
//! imports, and the report the preview and MCP show. Which file may be read at all is
//! `ssh_config_path`.
//!
//! - The report echoes no free text of the file beyond what a connection is made of (alias,
//!   address, user, port, key paths, jump host): a warning names the recognised keyword and the
//!   line, an unknown keyword only when it is a plausible one (ASCII letters, at most 32).
//! - A host becomes a connection named by its alias; `IdentityFile` gives the key path (the
//!   first file, by path - no key is read), auth method `key` with one and `agent` without, as
//!   `ssh` would try. `ProxyJump` and further key files go into the notes as not used.
//! - The repository's key is the alias in ASCII lower case, its source `ssh_config:<file>`.

use std::path::Path;

use rc_db::connections::{ImportCandidate, ImportResult, ImportStatus};
use serde::{Deserialize, Serialize};

use super::ssh_config::{ImportWarning, ImportedHost, SkippedHost, SshConfigImport, WarningKind};
use crate::{AuthMethod, ConnectionDraft, FolderId};

/// The root folder new connections go into when the import names none.
pub const DEFAULT_FOLDER: &str = "Imported from ssh config";

/// The prefix of the `import_source` an ssh_config import writes.
const SOURCE_PREFIX: &str = "ssh_config:";

/// What `connections.import_ssh_config` found, planned and did.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SshConfigReport {
    /// The file read.
    pub path: String,
    /// Whether this was only the plan.
    pub dry_run: bool,
    /// The folder new connections go into; `None` while the default one does not exist yet.
    pub folder_id: Option<FolderId>,
    /// Whether the import created that folder.
    pub folder_created: bool,
    /// Every connection candidate, in the file's order.
    pub hosts: Vec<SshConfigHost>,
    /// `Host` names that are no connection.
    pub skipped: Vec<SshConfigSkip>,
    pub warnings: Vec<SshConfigWarning>,
}

/// One candidate and what happens, or happened, to it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SshConfigHost {
    pub alias: String,
    /// `HostName`, or the alias.
    pub host: String,
    pub port: Option<u16>,
    pub user: Option<String>,
    /// Key files by path; the first one becomes the connection's key path.
    pub identity_files: Vec<String>,
    /// Not supported yet; kept in the notes of a new connection.
    pub proxy_jump: Option<String>,
    /// Keywords of the host that are not imported, when plausible ones; never their values.
    pub ignored_options: Vec<String>,
    pub file: String,
    pub line: usize,
    /// Whether this import writes it.
    pub selected: bool,
    /// `new`, `created`, `update`, `updated`, `unchanged` or `refused` (with a `code`).
    #[serde(flatten)]
    pub status: ImportStatus,
}

/// A `Host` name that is no connection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SshConfigSkip {
    pub name: String,
    /// `pattern` or `negated` ([`super::ssh_config::SkipReason::code`]).
    pub reason: String,
    pub file: String,
    pub line: usize,
}

/// Something the import did not take.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SshConfigWarning {
    /// [`WarningKind::code`]; the GUI's text is `ssh_import.warning.<code>`.
    pub code: String,
    pub file: String,
    pub line: usize,
    /// The keyword of the line, when recognised or plausible; never a value.
    pub keyword: Option<String>,
}

/// The longest keyword a report repeats.
const MAX_KEYWORD: usize = 32;

/// The `import_source` of the file at `path` (best given canonical).
#[must_use]
pub fn source_of(path: &Path) -> String {
    format!("{SOURCE_PREFIX}{}", path.display())
}

/// The repository's key of an alias.
#[must_use]
pub fn key_of(alias: &str) -> String {
    alias.to_ascii_lowercase()
}

/// The candidates of `found`, selected when `hosts` is `None` or names them (ignoring ASCII
/// case).
#[must_use]
pub fn candidates(found: &SshConfigImport, hosts: Option<&[String]>) -> Vec<ImportCandidate> {
    found
        .hosts
        .iter()
        .map(|host| ImportCandidate {
            key: key_of(&host.alias),
            draft: draft(host),
            selected: hosts.is_none_or(|names| {
                names
                    .iter()
                    .any(|name| name.trim().eq_ignore_ascii_case(&host.alias))
            }),
        })
        .collect()
}

/// The new connection a host describes.
fn draft(host: &ImportedHost) -> ConnectionDraft {
    let mut notes = Vec::new();
    if let Some(jump) = &host.proxy_jump {
        notes.push(format!("ProxyJump {jump} (not supported yet)"));
    }
    for file in host.identity_files.iter().skip(1) {
        notes.push(format!("IdentityFile {} (not used)", file.display()));
    }
    let key_path = host
        .identity_files
        .first()
        .map(|file| file.display().to_string());
    ConnectionDraft {
        port: host.port.map(i64::from),
        username: host.user.clone(),
        auth_method: if key_path.is_some() {
            AuthMethod::Key
        } else {
            AuthMethod::Agent
        },
        key_path,
        notes: (!notes.is_empty()).then(|| notes.join("\n")),
        ..ConnectionDraft::new(host.alias.clone(), host.address())
    }
}

/// The report of an import of `found`, read from `path`.
#[must_use]
pub fn report(
    path: String,
    dry_run: bool,
    found: &SshConfigImport,
    candidates: &[ImportCandidate],
    result: ImportResult,
) -> SshConfigReport {
    let hosts = found
        .hosts
        .iter()
        .zip(candidates)
        .zip(result.statuses)
        .map(|((host, candidate), status)| SshConfigHost {
            alias: host.alias.clone(),
            host: host.address().to_owned(),
            port: host.port,
            user: host.user.clone(),
            identity_files: host
                .identity_files
                .iter()
                .map(|file| file.display().to_string())
                .collect(),
            proxy_jump: host.proxy_jump.clone(),
            ignored_options: host
                .ignored_options
                .iter()
                .filter_map(|keyword| plausible_keyword(keyword))
                .collect(),
            file: host.source.file.display().to_string(),
            line: host.source.line,
            selected: candidate.selected,
            status,
        })
        .collect();
    SshConfigReport {
        path,
        dry_run,
        folder_id: result.folder,
        folder_created: result.folder_created,
        hosts,
        skipped: found.skipped.iter().map(skip).collect(),
        warnings: found.warnings.iter().map(warning).collect(),
    }
}

fn skip(skipped: &SkippedHost) -> SshConfigSkip {
    SshConfigSkip {
        name: skipped.name.clone(),
        reason: skipped.reason.code().to_owned(),
        file: skipped.source.file.display().to_string(),
        line: skipped.source.line,
    }
}

fn warning(warning: &ImportWarning) -> SshConfigWarning {
    let keyword = match &warning.kind {
        WarningKind::IncludeCycle { .. }
        | WarningKind::IncludeTooDeep { .. }
        | WarningKind::IncludeNotFound { .. }
        | WarningKind::IncludeUnreadable { .. }
        | WarningKind::IncludeInMatch
        | WarningKind::IncludeRefused { .. }
        | WarningKind::IncludeRepeated { .. }
        | WarningKind::TooManyIncludes => Some("Include".to_owned()),
        WarningKind::TooManyHosts => Some("Host".to_owned()),
        WarningKind::MatchSkipped => Some("Match".to_owned()),
        WarningKind::InvalidPort { .. } => Some("Port".to_owned()),
        WarningKind::OptionOutsideHost { keyword } | WarningKind::MissingArgument { keyword } => {
            plausible_keyword(keyword)
        }
        WarningKind::UnknownToken { .. }
        | WarningKind::UnterminatedQuote
        | WarningKind::MissingKeyword => None,
    };
    SshConfigWarning {
        code: warning.kind.code().to_owned(),
        file: warning.source.file.display().to_string(),
        line: warning.source.line,
        keyword,
    }
}

/// `keyword` when it looks like one - ASCII letters only - cut to [`MAX_KEYWORD`]; anything
/// else may be text of the file and is never repeated.
fn plausible_keyword(keyword: &str) -> Option<String> {
    (!keyword.is_empty() && keyword.bytes().all(|byte| byte.is_ascii_alphabetic()))
        .then(|| keyword.chars().take(MAX_KEYWORD).collect())
}

#[cfg(test)]
#[path = "ssh_config_apply_tests.rs"]
mod tests;
