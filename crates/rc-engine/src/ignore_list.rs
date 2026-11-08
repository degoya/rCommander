//! Total Commander's ignore list (RC-100-92): names and paths the panels never show, while the
//! switch `ignore_list.enabled` is on (`panel.toggle_ignore_list`). The list is the setting
//! `ignore_list.entries`, one entry per line, kept in the database (the backup carries it) rather
//! than in a file of its own.
//!
//! As Total Commander reads its `tcignore.txt`: an entry without a separator is a name or a mask
//! (`*.bak`, `Thumbs.db`) and hides that name in every folder, on every backend. An entry with a
//! drive or a share (`C:\Windows\Installer`, `\\nas\share\tmp`) is that path, `%VAR%` expanded
//! (`%USERPROFILE%\AppData`, as the Total Commander import expands it); one starting with `\`
//! without a drive (`\$RECYCLE.BIN`) is that path on every drive; another one with a separator
//! (`node_modules\.cache`) ends a path. Paths count on this machine only (WSL's shares included),
//! never on a server. A `\` at the end limits an entry to folders. Case is ignored.
//!
//! What it hides is left out of the file lists, the flat views, the search (Alt+F7) and the
//! folder sizes (Space, Alt+Shift+Enter); a copy or a move leaves it where it is
//! (`rc_ops::IgnoreRule`), and the delete question names how many a folder holds
//! ([`crate::Engine::ignored_below`]). The list in force is this process's ([`set_active`]), as
//! the name order is: the engine reads it at the start and on each change of the two settings.

use std::{
    path::Path,
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicBool, Ordering},
    },
};

use rc_vfs::{BackendId, Entry, VfsPath};

use crate::{
    import::total_commander_path::{environment, expand},
    mask::{lowercase, wildcard},
    recover::LockOrRecover as _,
};

/// The switch.
pub const IGNORE_ENABLED_KEY: &str = "ignore_list.enabled";
/// The entries, one per line.
pub const IGNORE_ENTRIES_KEY: &str = "ignore_list.entries";
/// The most lines the list takes.
pub const MAX_IGNORE_LINES: usize = 1000;
/// The most bytes the list takes.
pub const MAX_IGNORE_BYTES: usize = 64 * 1024;

/// What an entry is compared with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Scope {
    /// The entry's name, on every backend.
    Name,
    /// The entry's whole path on this machine.
    Path,
    /// The entry's path on this machine without its drive or share.
    Rooted,
}

/// One line of the list.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Rule {
    /// Lower case, `/` for every separator.
    pattern: Vec<char>,
    scope: Scope,
    /// The line ended in a separator: folders only.
    folders_only: bool,
}

/// The ignore list as read.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct IgnoreList {
    rules: Vec<Rule>,
}

impl IgnoreList {
    /// The list `text`, `%VAR%` resolved from this process's environment (only the variables the
    /// Total Commander import knows).
    #[must_use]
    pub fn parse(text: &str) -> Self {
        Self::parse_with(text, &|name| environment(name, None))
    }

    /// [`IgnoreList::parse`] with `lookup` answering the variables.
    pub(crate) fn parse_with(text: &str, lookup: &dyn Fn(&str) -> Option<String>) -> Self {
        let rules = text
            .lines()
            .take(MAX_IGNORE_LINES)
            .filter_map(|line| rule(&expand(line.trim(), lookup)))
            .collect();
        Self { rules }
    }

    /// Whether the list has no entry.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.rules.is_empty()
    }

    /// Whether `entry`, listed in the folder `dir`, is hidden.
    #[must_use]
    pub fn hides(&self, dir: &VfsPath, entry: &Entry) -> bool {
        let folder = entry.is_dir_like();
        let name: Vec<char> = lowercase(own_name(&entry.name)).collect();
        // The whole path only when a rule needs it, and only on this machine.
        let mut full: Option<Option<Vec<char>>> = None;
        self.rules.iter().any(|rule| {
            if rule.folders_only && !folder {
                return false;
            }
            match rule.scope {
                Scope::Name => wildcard(&rule.pattern, name.iter()),
                Scope::Path | Scope::Rooted => {
                    let Some(path) = full.get_or_insert_with(|| local_path(dir, &entry.name))
                    else {
                        return false;
                    };
                    let path: &[char] = path;
                    let path = if rule.scope == Scope::Rooted {
                        without_root(path)
                    } else {
                        path
                    };
                    wildcard(&rule.pattern, path.iter())
                }
            }
        })
    }
}

impl rc_ops::IgnoreRule for IgnoreList {
    fn ignores(&self, dir: &VfsPath, entry: &Entry) -> bool {
        self.hides(dir, entry)
    }
}

/// The rule of one line; `None` for a blank one.
fn rule(line: &str) -> Option<Rule> {
    let folders_only = line.ends_with(['\\', '/']);
    let text: String = lowercase(line.trim_end_matches(['\\', '/']))
        .map(|c| if c == '\\' { '/' } else { c })
        .collect();
    if text.is_empty() {
        return None;
    }
    let drive = rc_vfs::drive_letter(&text).is_some() || text.starts_with("//");
    let (scope, pattern) = if drive || (!cfg!(windows) && text.starts_with('/')) {
        (Scope::Path, text)
    } else if text.starts_with('/') {
        (Scope::Rooted, text)
    } else if text.contains('/') {
        (Scope::Path, format!("*/{text}"))
    } else {
        (Scope::Name, text)
    };
    Some(Rule {
        pattern: pattern.chars().collect(),
        scope,
        folders_only,
    })
}

/// The last part of a name that may carry folders (a flat view's `sub\file.txt`).
fn own_name(name: &str) -> &str {
    name.rsplit(['\\', '/']).next().unwrap_or(name)
}

/// The path of `name` in `dir` as the rules compare it - lower case, `/` for every separator,
/// no `\\?\` - when `dir` lies on this machine's disks.
fn local_path(dir: &VfsPath, name: &str) -> Option<Vec<char>> {
    if dir.backend != BackendId::Local {
        return None;
    }
    let joined = dir.path.join(name);
    let text = joined.to_string_lossy();
    let text = text.strip_prefix(r"\\?\").unwrap_or(&text);
    Some(
        lowercase(text)
            .map(|c| if c == '\\' { '/' } else { c })
            .collect(),
    )
}

/// `path` without its drive (`c:`) or share (`//host/share`).
fn without_root(path: &[char]) -> &[char] {
    if path.len() >= 2 && path[1] == ':' {
        return &path[2..];
    }
    if path.starts_with(&['/', '/']) {
        let mut slashes = 0;
        for (at, c) in path.iter().enumerate().skip(2) {
            if *c == '/' {
                slashes += 1;
                if slashes == 2 {
                    return &path[at..];
                }
            }
        }
        return &[];
    }
    path
}

/// Whether a list is in force: read without a lock by every listing.
static ON: AtomicBool = AtomicBool::new(false);

fn active_cell() -> &'static Mutex<Option<Arc<IgnoreList>>> {
    static ACTIVE: OnceLock<Mutex<Option<Arc<IgnoreList>>>> = OnceLock::new();
    ACTIVE.get_or_init(|| Mutex::new(None))
}

/// The list in force from now on, for the whole process; `None` (or an empty list) hides nothing.
pub fn set_active(list: Option<Arc<IgnoreList>>) {
    let list = list.filter(|list| !list.is_empty());
    ON.store(list.is_some(), Ordering::Relaxed);
    *active_cell().lock_or_recover("ignore_list.active") = list;
}

/// The list in force, if any.
#[must_use]
pub fn active() -> Option<Arc<IgnoreList>> {
    if !ON.load(Ordering::Relaxed) {
        return None;
    }
    active_cell().lock_or_recover("ignore_list.active").clone()
}

/// Whether the list in force hides `entry` of the folder `dir`.
pub(crate) fn ignored(dir: &VfsPath, entry: &Entry) -> bool {
    active().is_some_and(|list| list.hides(dir, entry))
}

/// `entries` of the folder `dir` without what the list in force hides.
pub(crate) fn retain_shown(dir: &VfsPath, entries: &mut Vec<Entry>) {
    if let Some(list) = active() {
        entries.retain(|entry| !list.hides(dir, entry));
    }
}

/// The rule of the list in force over the paths of `backend`, for a walk that knows paths only
/// (`rc_ops::dir_size_without`); `None` while no list is in force.
pub(crate) fn skip_on(backend: BackendId) -> Option<impl Fn(&Path, &Entry) -> bool + Send + Sync> {
    let list = active()?;
    Some(move |dir: &Path, entry: &Entry| {
        list.hides(
            &VfsPath {
                backend: backend.clone(),
                path: dir.to_path_buf(),
            },
            entry,
        )
    })
}

/// The list's text as the setting keeps it, or why it does not fit: at most
/// [`MAX_IGNORE_LINES`] lines and [`MAX_IGNORE_BYTES`] bytes, no control character but the line
/// breaks; lines trimmed, blank ones dropped, `\n` between them.
///
/// # Errors
///
/// The text is too long or holds a control character.
pub fn checked_lines(text: &str) -> Result<String, String> {
    if text.len() > MAX_IGNORE_BYTES {
        return Err(format!("expected at most {MAX_IGNORE_BYTES} bytes"));
    }
    let lines: Vec<&str> = text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect();
    if lines.len() > MAX_IGNORE_LINES {
        return Err(format!(
            "expected at most {MAX_IGNORE_LINES} lines, a name, a mask or a path on each"
        ));
    }
    if lines.iter().any(|line| line.chars().any(char::is_control)) {
        return Err("expected lines without control characters".to_owned());
    }
    Ok(lines.join("\n"))
}

#[cfg(test)]
#[path = "ignore_list_tests.rs"]
mod tests;
