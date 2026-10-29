//! The sensitive-path guard: files whose contents never leave through MCP, whatever the policy.
//!
//! [`sensitive_path`] names rCommander's own files (`rcommander.db` with its `-wal` and `-shm`,
//! backups and staged restores, `mcp-instance.json`, everything in the backup folder
//! [`rc_db::BACKUP_DIR`]), SSH and GnuPG keys, private keys and key stores, and credential files
//! (other clients' password stores, Total Commander's `wcx_ftp.ini` and Remote Desktop Manager's
//! `*.rdm` exports among them).
//! Every tool that answers with a file's contents or with data derived from them (a viewer
//! window, checksums, split parts) asks about each file first and refuses with `sensitive_path`;
//! the engine asks about every file a command finds by itself (a content search, a folder's
//! checksums, the sources of a copy; `rc_engine`'s read guard, attached in `server.rs`). Names
//! are compared ignoring case and also as Windows opens them (an alternate data stream and
//! trailing dots and spaces cut off: `key.pem.`, `rcommander.db::$DATA`), and a path is judged as
//! it really is (links and junctions followed) as well as by the name it was given; a copy of the
//! database under any other name is known by its content ([`rc_db::is_rcommander_database`]). A
//! person can still open such a file in the window. The second line of defence is the mask,
//! which replaces every private key block in every answer (`mask.rs`).

use std::{
    path::{Component, Path, PathBuf},
    sync::OnceLock,
};

use rc_engine::{
    Engine, PanelId, RDM_EXPORT_PASSWORDS, SSH_PRIVATE_FILE, TC_FTP_PASSWORDS, archive_file,
    shown_path,
};
use rmcp::model::CallToolResult;

use crate::{answer::error_result, instance::INSTANCE_FILE};

/// Files in `.ssh` an agent may read besides the public halves (`*.pub`).
const SSH_PUBLIC: &[&str] = &["config", "known_hosts"];

/// Extensions of private keys and key stores.
const KEY_EXTENSIONS: &[&str] = &["jks", "kdbx", "key", "keystore", "p12", "pem", "pfx", "ppk"];

/// Names of files that hold credentials wherever they are; WinSCP's portable configuration keeps
/// its stored passwords merely obfuscated (RC-080-03).
const CREDENTIAL_NAMES: &[&str] = &[
    ".env",
    ".git-credentials",
    ".netrc",
    ".pgpass",
    "_netrc",
    "winscp.ini",
];

/// Total Commander's FTP connections with their passwords, obfuscated with a fixed key stream
/// (RC-080-12).
const TC_FTP_INI: &str = "wcx_ftp.ini";

/// Remote Desktop Manager's exports, their passwords obfuscated with RDM's built-in key
/// (`SafePassword`, RC-050-11).
const RDM_EXTENSION: &str = "rdm";

/// Credential files known by their folder and name; FileZilla keeps its sites' passwords in
/// base64 unless a master password is set (RC-080-03).
const CREDENTIAL_PAIRS: &[(&str, &str)] = &[
    (".aws", "credentials"),
    (".docker", "config.json"),
    (".kube", "config"),
    ("etc", "gshadow"),
    ("etc", "shadow"),
    ("filezilla", "recentservers.xml"),
    ("filezilla", "sitemanager.xml"),
];

/// Why the file at `path` must not be read through MCP, or `None` when it may. Judged by its
/// name and folders, ignoring case; `refuse_sensitive` also judges where a link leads.
#[must_use]
pub fn sensitive_path(path: &Path) -> Option<&'static str> {
    let dirs = backup_dirs();
    judge(path, dirs).or_else(|| judge(&as_windows_opens(path)?, dirs))
}

/// `path` with every name as Windows opens it - an alternate data stream (`name:stream`,
/// `name::$DATA`) cut off, trailing dots and spaces dropped - or `None` when no name changes.
/// Judged besides the path as given, never instead: elsewhere `key.pem.` is another file.
pub(crate) fn as_windows_opens(path: &Path) -> Option<PathBuf> {
    let mut changed = false;
    let opened: PathBuf = path
        .components()
        .map(|component| match component {
            Component::Normal(name) => {
                let name = name.to_string_lossy();
                let stream = name.split(':').next().unwrap_or_default();
                let trimmed = stream.trim_end_matches(['.', ' ']);
                if trimmed.is_empty() || trimmed.len() == name.len() {
                    component.as_os_str().to_owned()
                } else {
                    changed = true;
                    trimmed.into()
                }
            }
            other => other.as_os_str().to_owned(),
        })
        .collect();
    changed.then_some(opened)
}

/// [`sensitive_path`] with the backup folders given, as written and as they really are.
fn judge(path: &Path, backup_dirs: &[PathBuf]) -> Option<&'static str> {
    let parts = lowered(path);
    if backup_dirs
        .iter()
        .map(PathBuf::as_path)
        .map(lowered)
        .any(|dir| !dir.is_empty() && parts.starts_with(&dir))
    {
        return Some("a backup of rCommander's data");
    }
    let name = path.file_name()?.to_string_lossy().to_lowercase();
    if rc_db::is_database_file_name(&name) || name == INSTANCE_FILE {
        return Some("rCommander's own data");
    }
    let folders = &parts[..parts.len().saturating_sub(1)];
    let inside = |folder: &str| folders.iter().any(|component| component == folder);
    if inside(".ssh") && !name.ends_with(".pub") && !SSH_PUBLIC.contains(&name.as_str()) {
        return Some(SSH_PRIVATE_FILE);
    }
    if inside(".gnupg") {
        return Some("a GnuPG key ring");
    }
    let extension = Path::new(&name)
        .extension()
        .map(|extension| extension.to_string_lossy().into_owned());
    if extension
        .as_deref()
        .is_some_and(|extension| KEY_EXTENSIONS.contains(&extension))
    {
        return Some("a private key or key store");
    }
    let parent = folders.last().map(String::as_str);
    // Readable to the import from Total Commander alone, which knows this reason.
    if name == TC_FTP_INI {
        return Some(TC_FTP_PASSWORDS);
    }
    // Readable to the import from Remote Desktop Manager alone, likewise.
    if extension.as_deref() == Some(RDM_EXTENSION) {
        return Some(RDM_EXPORT_PASSWORDS);
    }
    if CREDENTIAL_NAMES.contains(&name.as_str())
        || name.starts_with(".env.")
        || CREDENTIAL_PAIRS
            .iter()
            .any(|(folder, file)| parent == Some(*folder) && name == *file)
    {
        return Some("a file of credentials");
    }
    None
}

/// The path's folder and file names in lower case; root and drive prefix left out.
pub(crate) fn lowered(path: &Path) -> Vec<String> {
    path.components()
        .filter_map(|component| match component {
            Component::Normal(name) => Some(name.to_string_lossy().to_lowercase()),
            _ => None,
        })
        .collect()
}

/// The default backup folder, as the platform names it and with links resolved; empty when the
/// platform has no data directory.
fn backup_dirs() -> &'static [PathBuf] {
    static DIRS: OnceLock<Vec<PathBuf>> = OnceLock::new();
    DIRS.get_or_init(|| {
        let Some(dir) = rc_db::Db::default_path()
            .ok()
            .and_then(|db| db.parent().map(|data| data.join(rc_db::BACKUP_DIR)))
        else {
            return Vec::new();
        };
        let mut dirs = vec![dir.clone()];
        dirs.extend(std::fs::canonicalize(&dir).ok());
        dirs
    })
}

/// The refusal for the file `target` names (resolved against `panel`'s directory), when it is
/// sensitive - as it really is on the local backend (canonical, links followed), or by name.
pub(crate) async fn refuse_sensitive(
    engine: &Engine,
    panel: Option<PanelId>,
    target: &str,
) -> Option<CallToolResult> {
    refuse_judged(engine, panel, target, sensitive_path).await
}

/// [`refuse_sensitive`] for a target that may be a folder whose files a tool reads: refused as
/// well when the files inside it would be (`.ssh`, `.gnupg`, the backup folder).
pub(crate) async fn refuse_sensitive_tree(
    engine: &Engine,
    panel: Option<PanelId>,
    target: &str,
) -> Option<CallToolResult> {
    refuse_judged(engine, panel, target, |path| {
        sensitive_path(path).or_else(|| sensitive_path(&path.join(TREE_PROBE)))
    })
    .await
}

/// A file name no rule but a folder's matches: what [`refuse_sensitive_tree`] asks about a
/// folder's contents with.
const TREE_PROBE: &str = "rcommander-probe";

async fn refuse_judged(
    engine: &Engine,
    panel: Option<PanelId>,
    target: &str,
    judge: impl Fn(&Path) -> Option<&'static str>,
) -> Option<CallToolResult> {
    let path = engine.resolve_target(panel, target);
    // Inside an archive the archive file is judged as well: a key store packed into a zip, or a
    // backup's archive, stays what it is.
    let mut judged = Some(&path);
    while let Some(at) = judged {
        // Where it really leads first, then the name it was given (a link's or a missing file's).
        let real = engine.link_target(at).await;
        let mut reason = real.as_deref().and_then(&judge).or_else(|| judge(&at.path));
        // A copy of the database under any other name is known by its content (fails closed).
        if reason.is_none()
            && let Some(real) = real.as_deref()
            && rc_db::is_rcommander_database(real).await
        {
            reason = Some("rCommander's own data");
        }
        if let Some(reason) = reason {
            return Some(error_result(
                "sensitive_path",
                &format!(
                    "{} is {reason}: its contents are never shown through MCP. A person can open \
                     it in rCommander.",
                    shown_path(&path).display()
                ),
            ));
        }
        judged = archive_file(at);
    }
    None
}

#[cfg(test)]
#[path = "guard_tests.rs"]
mod tests;
