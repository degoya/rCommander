//! The write-target guard (RC-080-09): places no MCP call writes to, whatever the policy.
//!
//! [`protected_target`] names rCommander's data folder (the database, a staged restore and its
//! mark, the instance file, the backup folder and the update's folder inside it), the program's
//! own folder (RC-110-19: a file planted beside the program runs or is loaded at the next start),
//! the folders an update keeps beside the program (`.update-*`, `.previous`, `.trash`,
//! `.failed-*`, RC-100-06), rCommander's own file names anywhere
//! (`rc_db::is_database_file_name`, `mcp-instance.json`, and the portable mode's marker
//! `rcommander.portable`, which would start the next run on an empty data folder), every `.ssh` folder (`authorized_keys`,
//! `config` - locally and on a server), the autostart folders (Windows' `Start Menu\Programs\
//! Startup` of the user and of all users, `~/.config/autostart`, systemd's user units in
//! `~/.config/systemd/user`), shells' start-up files (`.bashrc`, `.profile`, `.bash_logout`,
//! fish's `config.fish`, PowerShell's `profile.ps1`, ...), and what a tool runs on its own: Git's
//! hooks (`.git/hooks`) and its configuration (`.gitconfig`, which names programs Git starts),
//! and an editor's tasks (`.vscode/tasks.json`; the owner's list of 2026-10-28, RC-080-42). A
//! file written there replaces the database at the next start, runs at the next login or the
//! next commit, or decides who may log in - writing would amount to running a program without
//! the `exec` question. Every tool that writes asks about
//! each place before the engine is called and refuses with `protected_target`: a refusal, not a
//! question; a person does it in the window.
//!
//! A place is judged as written and as it really is on the local backend (the links and
//! junctions of its existing part followed), names ignoring case and as Windows opens them
//! (`guard.rs`). What a copy or an unpack writes below its target is judged entry by entry: a
//! local folder's tree ([`WriteGuard::refuse_copy`]) and an archive's entries
//! ([`WriteGuard::refuse_unpack`]); a folder on a server is judged by its own name.
//!
//! The same places are no source of a move or a rename (RC-110-19): taking the program, a file
//! of the data folder, an autostart entry or a shell's start-up file away breaks or changes the
//! next start as writing there does, and makes room for another file under the old name. A
//! folder that holds such a place is refused as well ([`WriteGuard::refuse_taking`]); a rename
//! is judged by names alone ([`WriteGuard::refuse_renaming`]: the entry, and a folder above the
//! program's or the data folder), a move also by the entries below a local folder. Deleting
//! is not judged here: every deleting tool is `destructive`, so a person is asked and the
//! question names the entries; copying and reading are the read guard's (`guard.rs`).

use std::path::{Path, PathBuf};

use rc_db::Db;
use rc_engine::{Command, Engine, EntryKind, Outcome, PanelId, VfsPath, shown_path};
use rmcp::model::CallToolResult;

use crate::{RcMcpServer, answer::error_result};

mod places;

pub(crate) use places::program_dirs;
pub use places::protected_target;
use places::{default_data_dirs, judge_all, judge_holding, with_real};
#[cfg(test)]
use places::{judge, program_dirs_of};

/// Whether `at` is on the local backend.
fn is_local(at: &VfsPath) -> bool {
    at.backend == VfsPath::local(PathBuf::new()).backend
}

/// Where `at` really is on the local backend: its longest existing part with links and junctions
/// resolved, the rest appended; `None` on another backend or when no part of it exists.
async fn real_path(at: &VfsPath) -> Option<PathBuf> {
    if !is_local(at) {
        return None;
    }
    let mut existing = at.path.as_path();
    let mut rest = Vec::new();
    loop {
        if let Ok(real) = tokio::fs::canonicalize(existing).await {
            return Some(rest.iter().rev().fold(real, |path, name| path.join(name)));
        }
        rest.push(existing.file_name()?);
        existing = existing.parent()?;
    }
}

/// The refusal of a write at `at`.
fn refusal(at: &VfsPath, reason: &str) -> CallToolResult {
    error_result(
        "protected_target",
        &format!(
            "{} is or lies in {reason}: MCP never writes there. A person can do it in rCommander.",
            shown_path(at).display()
        ),
    )
}

/// The refusal of a move or rename that would take `at` away, as [`WriteGuard::refuse_taking`]
/// judges it: `holds` when `at` is not in the place named by `reason` but a folder holding it.
fn refusal_taken(at: &VfsPath, reason: &str, holds: bool) -> CallToolResult {
    let shown = shown_path(at);
    let shown = shown.display();
    let text = if holds {
        format!(
            "{shown} holds {reason}: MCP never moves or renames it away. A person can do it in \
             rCommander."
        )
    } else {
        format!(
            "{shown} is or lies in {reason}: MCP never moves or renames anything away from \
             there. A person can do it in rCommander."
        )
    };
    error_result("protected_target", &text)
}

/// The place a call writes to that is named relative to the other panel: `target` resolved
/// against the other panel's directory, or that directory when it is omitted - inside
/// `Engine::pin_mcp_panels` the folder pinned, the one the command resolves against too
/// (RC-080-42).
pub(crate) fn other_panel_target(
    engine: &Engine,
    panel: Option<PanelId>,
    target: Option<&str>,
) -> VfsPath {
    let other = panel.unwrap_or_else(|| engine.active_panel()).other();
    match target {
        Some(target) => engine.resolve_target(Some(other), target),
        None => engine.mcp_folder(other),
    }
}

/// The guard of one call: the default data folder, the folder of the database the server runs
/// on and the program's folder.
pub(crate) struct WriteGuard {
    data_dirs: Vec<PathBuf>,
    program_dirs: Vec<PathBuf>,
}

impl WriteGuard {
    pub(crate) fn new(db: &Db, program_dirs: &[PathBuf]) -> Self {
        let mut data_dirs = default_data_dirs().to_vec();
        if let Some(dir) = db.path().and_then(Path::parent) {
            data_dirs.extend(with_real(dir.to_path_buf()));
        }
        Self {
            data_dirs,
            program_dirs: program_dirs.to_vec(),
        }
    }

    /// [`judge_all`] against this guard's folders.
    fn judge(&self, path: &Path) -> Option<&'static str> {
        judge_all(path, &self.data_dirs, &self.program_dirs)
    }

    /// Why nothing may be written at `at`: as it really is, then as written.
    pub(crate) async fn refuses(&self, at: &VfsPath) -> Option<&'static str> {
        if let Some(real) = real_path(at).await
            && let Some(reason) = self.judge(&real)
        {
            return Some(reason);
        }
        self.judge(&at.path)
    }

    /// [`WriteGuard::refuses`] as a tool's answer.
    pub(crate) async fn refuse(&self, at: &VfsPath) -> Option<CallToolResult> {
        let reason = self.refuses(at).await?;
        Some(refusal(at, reason))
    }

    /// The refusal of the first of `places` that is protected.
    pub(crate) async fn refuse_any(
        &self,
        places: impl IntoIterator<Item = VfsPath>,
    ) -> Option<CallToolResult> {
        for at in places {
            if let Some(refusal) = self.refuse(&at).await {
                return Some(refusal);
            }
        }
        None
    }

    /// [`WriteGuard::refuse`] for `dest`, the copy of `source`, and for every entry below a
    /// local folder `source` as it lands below `dest`. Links are not entered, as the read guard
    /// does not enter them.
    pub(crate) async fn refuse_copy(
        &self,
        source: &VfsPath,
        dest: &VfsPath,
    ) -> Option<CallToolResult> {
        self.refuse_transfer(source, dest, false).await
    }

    /// [`WriteGuard::refuse_copy`] for a move, which also takes `source` away
    /// ([`WriteGuard::refuse_taking`]); the source's tree is read once for both.
    pub(crate) async fn refuse_move(
        &self,
        source: &VfsPath,
        dest: &VfsPath,
    ) -> Option<CallToolResult> {
        self.refuse_transfer(source, dest, true).await
    }

    async fn refuse_transfer(
        &self,
        source: &VfsPath,
        dest: &VfsPath,
        moving: bool,
    ) -> Option<CallToolResult> {
        if let Some(refusal) = self.refuse(dest).await {
            return Some(refusal);
        }
        if moving && let Some(refusal) = self.refuse_renaming(source).await {
            return Some(refusal);
        }
        if !is_local(source) {
            return None;
        }
        let rels = local_tree(&source.path).await;
        if let Some(refusal) = self.refuse_below(dest, &rels).await {
            return Some(refusal);
        }
        if moving {
            return self.refuse_taking_below(source, &rels).await;
        }
        None
    }

    /// The refusal of a move or rename that takes the entry `at` away from where it is
    /// (RC-110-19): an entry in a protected place - the program moved aside, a file of the data
    /// folder, an autostart entry, a shell's start-up file - and a folder that holds one: the
    /// program's or the data folder anywhere below it, or, for a local folder, an entry below it
    /// that is protected by its name. Judged as written, as it really is and as Windows opens
    /// it, like a target. Copying and reading from there are not this guard's.
    pub(crate) async fn refuse_taking(&self, at: &VfsPath) -> Option<CallToolResult> {
        if let Some(refusal) = self.refuse_renaming(at).await {
            return Some(refusal);
        }
        if !is_local(at) {
            return None;
        }
        let rels = local_tree(&at.path).await;
        self.refuse_taking_below(at, &rels).await
    }

    /// [`WriteGuard::refuse_taking`] by the name of `at` alone: in a protected place, or a
    /// folder above the program's or the data folder. All a rename is judged by: what lies
    /// below a renamed folder stays in it, so its tree is not read - a folder with a Git
    /// repository in it is renamed like any other.
    pub(crate) async fn refuse_renaming(&self, at: &VfsPath) -> Option<CallToolResult> {
        if let Some(reason) = self.refuses(at).await {
            return Some(refusal_taken(at, reason, false));
        }
        let real = real_path(at).await;
        std::iter::once(&at.path)
            .chain(real.as_ref())
            .find_map(|path| judge_holding(path, &self.data_dirs, &self.program_dirs))
            .map(|reason| refusal_taken(at, reason, true))
    }

    /// [`WriteGuard::refuse_taking`] for the entries `rels` below the local folder `at`: the
    /// first that is protected where it is now.
    async fn refuse_taking_below(&self, at: &VfsPath, rels: &[PathBuf]) -> Option<CallToolResult> {
        let real = real_path(at).await;
        for rel in rels {
            for base in std::iter::once(&at.path).chain(real.as_ref()) {
                if let Some(reason) = self.judge(&base.join(rel)) {
                    return Some(refusal_taken(at, reason, true));
                }
            }
        }
        None
    }

    /// [`WriteGuard::refuse`] for every place unpacking `archive` (named relative to `panel`)
    /// into `dest` writes to: the archive's own folder (`own_folder`, judged under every name it
    /// can get) and each entry below it, with or without its folders. An archive that cannot
    /// be listed is answered with the engine's error: unpacking it would fail the same way.
    pub(crate) async fn refuse_unpack(
        &self,
        engine: &Engine,
        panel: Option<PanelId>,
        archive: &str,
        unpack: Unpack<'_>,
    ) -> Option<CallToolResult> {
        let rels = match archive_tree(engine, panel, archive).await {
            Ok(rels) => rels,
            Err(answer) => return Some(answer),
        };
        let rels: Vec<PathBuf> = if unpack.keep_paths {
            rels
        } else {
            rels.iter()
                .filter_map(|rel| rel.file_name().map(PathBuf::from))
                .collect()
        };
        let roots = if unpack.own_folder {
            let name = Path::new(archive)
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default();
            folder_names(&name)
                .into_iter()
                .map(|folder| unpack.dest.join(folder))
                .collect()
        } else {
            vec![unpack.dest.clone()]
        };
        for root in &roots {
            if let Some(refusal) = self.refuse(root).await {
                return Some(refusal);
            }
            if let Some(refusal) = self.refuse_below(root, &rels).await {
                return Some(refusal);
            }
        }
        None
    }

    /// The refusal for the first of `rels` below `root` that is protected; `root`'s real place
    /// is resolved once and the entries are judged by name.
    async fn refuse_below(&self, root: &VfsPath, rels: &[PathBuf]) -> Option<CallToolResult> {
        let real = real_path(root).await;
        for rel in rels {
            for base in std::iter::once(&root.path).chain(real.as_ref()) {
                if let Some(reason) = self.judge(&base.join(rel)) {
                    return Some(refusal(&root.join(rel), reason));
                }
            }
        }
        None
    }
}

/// Where and how `archives_extract` unpacks.
#[derive(Clone, Copy)]
pub(crate) struct Unpack<'a> {
    pub(crate) dest: &'a VfsPath,
    pub(crate) own_folder: bool,
    pub(crate) keep_paths: bool,
}

/// Every name an archive's own folder can get: its file name with any number of endings cut
/// (`site.tar.gz`, `site.tar`, `site`) - more than the unpacker cuts, never fewer.
fn folder_names(file: &str) -> Vec<String> {
    let mut names = vec![file.to_owned()];
    let mut name = file;
    while let Some((stem, _)) = name.rsplit_once('.') {
        if stem.is_empty() {
            break;
        }
        names.push(stem.to_owned());
        name = stem;
    }
    names
}

/// The paths of every entry below the local folder `dir`, relative to it; nothing when it is
/// no folder. Links are listed, not entered.
async fn local_tree(dir: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut stack = vec![PathBuf::new()];
    while let Some(rel) = stack.pop() {
        let Ok(mut entries) = tokio::fs::read_dir(dir.join(&rel)).await else {
            continue;
        };
        while let Ok(Some(entry)) = entries.next_entry().await {
            let path = rel.join(entry.file_name());
            if entry.file_type().await.is_ok_and(|kind| kind.is_dir()) {
                stack.push(path.clone());
            }
            found.push(path);
        }
    }
    found
}

/// The paths of every entry of the archive `archive` (relative to `panel`), relative to its
/// root, listed as `archives_list` lists them.
async fn archive_tree(
    engine: &Engine,
    panel: Option<PanelId>,
    archive: &str,
) -> Result<Vec<PathBuf>, CallToolResult> {
    let mut found = Vec::new();
    let mut stack: Vec<Option<PathBuf>> = vec![None];
    while let Some(dir) = stack.pop() {
        let listed = engine
            .dispatch_mcp(Command::ArchiveList {
                panel,
                path: Some(archive.to_owned()),
                dir: dir.as_ref().map(|dir| dir.to_string_lossy().into_owned()),
            })
            .await;
        let entries = match listed {
            Ok(Outcome::ArchiveListing { entries, .. }) => entries,
            Ok(_) => break,
            // As `respond` answers an engine error.
            Err(error) => return Err(error_result(error.code(), &error.to_string())),
        };
        for item in entries {
            let path = dir
                .as_deref()
                .unwrap_or(Path::new(""))
                .join(&item.entry.name);
            if item.entry.kind == EntryKind::Dir {
                stack.push(Some(path.clone()));
            }
            found.push(path);
        }
    }
    Ok(found)
}

impl RcMcpServer {
    /// The write guard of a call.
    pub(crate) fn write_guard(&self) -> WriteGuard {
        WriteGuard::new(&self.context.db, &self.context.program_dirs)
    }
}

#[cfg(test)]
#[path = "guard_write_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "guard_write_program_tests.rs"]
mod program_tests;

#[cfg(test)]
#[path = "guard_write_source_tests.rs"]
mod source_tests;
