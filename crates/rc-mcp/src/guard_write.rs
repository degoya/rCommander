//! The write-target guard (RC-080-09): places no MCP call writes to, whatever the policy.
//!
//! [`protected_target`] names rCommander's data folder (the database, a staged restore and its
//! mark, the instance file, the backup folder and the update's folder inside it), the folders an
//! update keeps beside the program (`.update-*`, `.previous`, `.trash`, `.failed-*`, RC-100-06),
//! rCommander's own file names anywhere
//! (`rc_db::is_database_file_name`, `mcp-instance.json`), every `.ssh` folder (`authorized_keys`,
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

use std::{
    path::{Path, PathBuf},
    sync::OnceLock,
};

use rc_db::Db;
use rc_engine::{Command, Engine, EntryKind, Outcome, PanelId, VfsPath, shown_path};
use rmcp::model::CallToolResult;

use crate::{
    RcMcpServer,
    answer::error_result,
    guard::{as_windows_opens, lowered},
    instance::INSTANCE_FILE,
};

/// Folder chains whose files start with the session, in lower case: Windows' Startup folders
/// (`%APPDATA%\Microsoft\Windows\...` and `%ProgramData%\...`) and the XDG autostart folder.
const AUTOSTART: &[&[&str]] = &[
    &["start menu", "programs", "startup"],
    &[".config", "autostart"],
    &[".config", "systemd", "user"],
];

/// Folder chains, in lower case, whose files a tool runs or obeys on its own, with the reason:
/// Git runs its hooks, fish its start-up file, an editor its tasks.
const RUN_BY_TOOLS: &[(&[&str], &str)] = &[
    (&[".git", "hooks"], "Git's hooks"),
    (&["fish", "config.fish"], "a shell's start-up file"),
    (&[".vscode", "tasks.json"], "an editor's tasks"),
];

/// File names a tool reads its programs from wherever they are: Git's configuration names an
/// editor, a pager and hooks it starts.
const TOOL_CONFIG: &[&str] = &[".gitconfig"];

/// Shells' start-up files, run by the next shell or login; PowerShell's profiles end in
/// `profile.ps1`.
const SHELL_STARTUP: &[&str] = &[
    ".bash_login",
    ".bash_logout",
    ".bash_profile",
    ".bashrc",
    ".cshrc",
    ".login",
    ".profile",
    ".tcshrc",
    ".zlogin",
    ".zprofile",
    ".zshenv",
    ".zshrc",
];

/// Why MCP must not write at `path`, or `None` when it may; judged by its name and folders
/// against the default data folder.
#[must_use]
pub fn protected_target(path: &Path) -> Option<&'static str> {
    judge_all(path, default_data_dirs())
}

/// [`judge`] of the path as given and as Windows opens it.
fn judge_all(path: &Path, data_dirs: &[PathBuf]) -> Option<&'static str> {
    judge(path, data_dirs).or_else(|| judge(&as_windows_opens(path)?, data_dirs))
}

/// [`protected_target`] with the data folders given.
fn judge(path: &Path, data_dirs: &[PathBuf]) -> Option<&'static str> {
    let parts = lowered(path);
    if data_dirs
        .iter()
        .map(|dir| lowered(dir))
        .any(|dir| !dir.is_empty() && parts.starts_with(&dir))
    {
        return Some("rCommander's data folder");
    }
    if parts
        .iter()
        .any(|name| rc_db::is_database_file_name(name) || name == INSTANCE_FILE)
    {
        return Some("one of rCommander's own files");
    }
    if parts.iter().any(|name| name == ".ssh") {
        return Some("an SSH folder");
    }
    // The folders an update keeps beside the program (RC-100-06): the staging, the previous
    // version, the trash and a rolled-back version; the data folder's `update/` is covered above.
    if parts.iter().any(|name| {
        name == ".previous"
            || name == ".trash"
            || name.starts_with(".update-")
            || name.starts_with(".failed-")
    }) {
        return Some("an update's own folder");
    }
    let holds = |chain: &[&str]| {
        parts.windows(chain.len()).any(|window| {
            window
                .iter()
                .zip(chain.iter())
                .all(|(name, want)| name == want)
        })
    };
    if AUTOSTART.iter().any(|chain| holds(chain)) {
        return Some("an autostart folder");
    }
    if let Some((_, reason)) = RUN_BY_TOOLS.iter().find(|(chain, _)| holds(chain)) {
        return Some(*reason);
    }
    if parts
        .iter()
        .any(|name| TOOL_CONFIG.contains(&name.as_str()))
    {
        return Some("Git's configuration");
    }
    let name = parts.last()?;
    if SHELL_STARTUP.contains(&name.as_str()) || name.ends_with("profile.ps1") {
        return Some("a shell's start-up file");
    }
    None
}

/// The default data folder, as the platform names it and with links resolved; empty when the
/// platform has no data directory.
fn default_data_dirs() -> &'static [PathBuf] {
    static DIRS: OnceLock<Vec<PathBuf>> = OnceLock::new();
    DIRS.get_or_init(|| {
        Db::default_path()
            .ok()
            .and_then(|db| db.parent().map(Path::to_path_buf))
            .map(with_real)
            .unwrap_or_default()
    })
}

/// `dir` and, when it differs and exists, where it really is.
fn with_real(dir: PathBuf) -> Vec<PathBuf> {
    let real = std::fs::canonicalize(&dir).ok();
    let mut dirs = vec![dir];
    dirs.extend(real);
    dirs
}

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

/// The guard of one call: the default data folder and the folder of the database the server
/// runs on.
pub(crate) struct WriteGuard {
    data_dirs: Vec<PathBuf>,
}

impl WriteGuard {
    pub(crate) fn new(db: &Db) -> Self {
        let mut data_dirs = default_data_dirs().to_vec();
        if let Some(dir) = db.path().and_then(Path::parent) {
            data_dirs.extend(with_real(dir.to_path_buf()));
        }
        Self { data_dirs }
    }

    /// Why nothing may be written at `at`: as it really is, then as written.
    pub(crate) async fn refuses(&self, at: &VfsPath) -> Option<&'static str> {
        if let Some(real) = real_path(at).await
            && let Some(reason) = judge_all(&real, &self.data_dirs)
        {
            return Some(reason);
        }
        judge_all(&at.path, &self.data_dirs)
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
        if let Some(refusal) = self.refuse(dest).await {
            return Some(refusal);
        }
        if !is_local(source) {
            return None;
        }
        let rels = local_tree(&source.path).await;
        self.refuse_below(dest, &rels).await
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
                if let Some(reason) = judge_all(&base.join(rel), &self.data_dirs) {
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
        WriteGuard::new(&self.context.db)
    }
}

#[cfg(test)]
#[path = "guard_write_tests.rs"]
mod tests;
