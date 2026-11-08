//! A change kept over a restart (owner, 2026-11-07): quitting never waits and never uploads.
//! The copies whose change is not on the server stay with their session files, the others go
//! ([`Engine::finish_remote_edits`]). The next start takes each kept one up again
//! ([`Engine::restore_remote_edits`]) and asks Upload - checked against its baseline like any
//! upload - Discard or Later; a copy that turns out unchanged goes without a word.

use std::{
    fs,
    path::{Path, PathBuf},
};

use super::{
    Edit, RemoteEditChange, RemoteEditQuestion, RemoteEditState, Stamp, folder, is_server, rules,
    watch::Watch,
};
use crate::recover::LogPanic as _;
use crate::{Engine, check_name, unpacked};

/// A kept copy found at the start: its folder (now this process's), its session, its stamp.
type Found = (PathBuf, folder::Session, Option<Stamp>);

impl Engine {
    /// Takes up the copies an ended run kept with a change, each with the question Upload,
    /// Discard or Later; how many. Unchanged copies, and folders whose session does not read,
    /// are removed.
    pub async fn restore_remote_edits(&self) -> usize {
        let Some(root) = self.edit_root() else {
            return 0;
        };
        let found = tokio::task::spawn_blocking(move || take_kept(&root, std::process::id()))
            .await
            .log_panic("remote_edit.restore")
            .unwrap_or_default();
        let mut offered = 0;
        for (dir, session, stamp) in found {
            let edit = Edit {
                file: dir.join(&session.name),
                dir,
                shown: session.shown,
                remote: session.remote,
                server: session.server,
                name: session.name,
                baseline: session.baseline,
                in_step: session.synced,
                watch: Watch::new(stamp),
                in_place: session.in_place,
                state: RemoteEditState::Changed,
                question: Some(RemoteEditQuestion::Kept),
                uploading: false,
                again: false,
            };
            if let Ok(id) = self.adopt_edit(edit).await {
                self.announce_edit(id, RemoteEditChange::Asked);
                offered += 1;
            }
        }
        offered
    }

    /// At quit: the copies with a change the server does not have stay with their session
    /// files for the next start, the others go; how many stay. Nothing is uploaded and nothing
    /// waited for. Blocking, a few small files: the window calls it on its way out.
    pub fn finish_remote_edits(&self) -> usize {
        let edits = std::mem::take(&mut self.edits().edits);
        let mut kept = 0;
        for edit in edits.into_values() {
            let changed = edit.state.unsaved()
                || edit.question == Some(RemoteEditQuestion::Kept)
                || rules::local_stamp(&edit.file) != edit.in_step;
            if changed && folder::write_session(&edit.dir, &edit.session()).is_ok() {
                kept += 1;
            } else {
                folder::remove(&edit.dir);
            }
        }
        kept
    }
}

/// The kept copies under `root` of ended processes other than `own`, each moved into a folder
/// named after `own` - of two starts at once only one takes it. Blocking.
fn take_kept(root: &Path, own: u32) -> Vec<Found> {
    let mut found = Vec::new();
    for dir in folder::folders(root, |pid| pid != own && !unpacked::alive(pid)) {
        let Some(session) = folder::read_session(&dir) else {
            folder::remove(&dir);
            continue;
        };
        let valid = check_name(&session.name).is_ok()
            && is_server(&session.remote.backend)
            && session.shown.backend == session.remote.backend;
        let stamp = rules::local_stamp(&dir.join(&session.name));
        if !valid || stamp.is_none() || stamp == session.synced {
            folder::remove(&dir);
            continue;
        }
        if let Some(moved) = take_over(&dir, own) {
            found.push((moved, session, stamp));
        }
    }
    found
}

/// Renames the folder `dir` (`edit-<pid>-<random>`) and its session file to `own`'s name; the
/// new folder, or `None` when another start took it first.
fn take_over(dir: &Path, own: u32) -> Option<PathBuf> {
    let name = dir.file_name()?.to_str()?;
    let random = name.strip_prefix(folder::PREFIX)?.split_once('-')?.1;
    let moved = dir.with_file_name(format!("{}{own}-{random}", folder::PREFIX));
    fs::rename(dir, &moved).ok()?;
    if fs::rename(folder::session_path(dir), folder::session_path(&moved)).is_err() {
        // The session stayed behind: put the folder back for a later start.
        let _ = fs::rename(&moved, dir);
        return None;
    }
    Some(moved)
}

#[cfg(test)]
#[path = "kept_tests.rs"]
mod tests;
