//! What the window shows of archives (RC-030-02): the path bar through an archive file, the
//! marks and the packed size of an entry inside one, and the result of an archive test.
//!
//! An archive is opened, left and read by the engine (`file.open`, `panel.open_entry`,
//! `panel.navigate`); the window only draws where the panel is and what the archive says.

use std::path::{Component, Path, PathBuf};

use rc_engine::{
    ArchiveEntryInfo, JobChange, JobId, JobKind, JobSnapshot, JobState, VfsPath, archive_file,
};

use super::{App, path_bar::breadcrumb};
use crate::i18n::t;

/// The segments of the path bar for `path`, each with what a click navigates to. Inside an
/// archive: the folders outside it (reached by `..` steps, which leave the archive at its root),
/// the archive file marked as one (its root, `/`), then the folders inside it.
pub(super) fn breadcrumb_of(path: &VfsPath) -> Vec<(String, PathBuf)> {
    let Some(outer) = archive_file(path) else {
        return breadcrumb(&path.path);
    };
    let inside: Vec<String> = path
        .path
        .components()
        .filter_map(|component| match component {
            Component::Normal(name) => Some(name.to_string_lossy().into_owned()),
            _ => None,
        })
        .collect();
    let up = |levels: usize| -> PathBuf { (0..levels).map(|_| "..").collect() };
    let outside = breadcrumb(outer.path.parent().unwrap_or(Path::new("")));
    let count = outside.len();
    let mut segments: Vec<(String, PathBuf)> = outside
        .into_iter()
        .enumerate()
        .map(|(index, (label, _))| (label, up(inside.len() + count - index)))
        .collect();
    let name = outer
        .path
        .file_name()
        .map_or_else(String::new, |name| name.to_string_lossy().into_owned());
    segments.push((t!("path_bar.archive", name = name), PathBuf::from("/")));
    let mut so_far = PathBuf::from("/");
    for name in inside {
        so_far.push(&name);
        segments.push((name, so_far.clone()));
    }
    segments
}

/// The marks behind an entry's name inside an archive, and whether its stored name was unsafe
/// (drawn as a warning).
pub(super) fn marks(info: Option<&ArchiveEntryInfo>) -> (String, bool) {
    let Some(info) = info else {
        return (String::new(), false);
    };
    let mut marks = String::new();
    if info.encrypted {
        marks.push_str(t!("archive.mark.encrypted"));
    }
    if info.unsafe_name.is_some() {
        marks.push_str(t!("archive.mark.unsafe"));
    }
    (marks, info.unsafe_name.is_some())
}

/// The packed size of an entry inside an archive, where the format tells it per entry.
pub(super) fn packed(info: Option<&ArchiveEntryInfo>) -> String {
    info.and_then(|info| info.packed_size)
        .map_or_else(String::new, crate::format::size)
}

/// The line an archive test leaves in the status bar: passed, damaged and encrypted entries,
/// the first damaged ones by name. `None` for any other job.
pub(super) fn test_summary(job: &JobSnapshot) -> Option<String> {
    let JobKind::TestArchive { archive } = &job.kind else {
        return None;
    };
    let name = archive_file(archive)
        .and_then(|file| file.path.file_name())
        .map_or_else(String::new, |name| name.to_string_lossy().into_owned());
    let (encrypted, damaged): (Vec<_>, Vec<_>) = job
        .failures
        .iter()
        .partition(|failure| failure.code == "password_required");
    let failed = job.failures.len() as u64;
    let passed = job
        .progress
        .files_done
        .saturating_sub(failed)
        .saturating_sub(job.skipped);
    let first: Vec<String> = damaged
        .iter()
        .take(3)
        .filter_map(|failure| failure.path.path.file_name())
        .map(|name| name.to_string_lossy().into_owned())
        .collect();
    let state = match job.state {
        JobState::Completed => t!("archive.test.completed"),
        JobState::Cancelled => t!("archive.test.cancelled"),
        _ => t!("archive.test.failed"),
    };
    let first = if first.is_empty() {
        String::new()
    } else {
        t!("archive.test.first", names = first.join(", "))
    };
    Some(t!(
        "archive.test.result",
        name = name,
        state = state,
        passed = passed,
        damaged = damaged.len(),
        encrypted = encrypted.len(),
        first = first,
    ))
}

impl App {
    /// A job's end: an archive test's result goes to the status bar.
    pub(super) fn archive_test_result(&mut self, job: JobId, change: &JobChange) {
        if let JobChange::Finished { .. } = change
            && let Ok(snapshot) = self.engine.ops().job(job)
            && let Some(summary) = test_summary(&snapshot)
        {
            self.info = Some(summary);
        }
    }
}

#[cfg(test)]
#[path = "archive_view_tests.rs"]
mod tests;
