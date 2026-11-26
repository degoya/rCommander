//! The texts that name more than one number (RC-110-91). A pair `<key>.one`/`<key>.other` takes
//! its form from one number, its `{count}` (RC-110-82); a text with several numbers a word agrees
//! with is a frame `<frame>` whose placeholders are parts, each a pair `<frame>.<part>` of its own
//! chosen by its own number, so "1 file, 2 folders" reads right in every language. The frame puts
//! the parts in the order its language reads them. The repo lint `counted_texts` keeps a number
//! from going into a text any other way.

use crate::{
    format,
    i18n::t,
    locale::{self, Lang},
};

/// The texts with several numbers in one language: the interface's ([`Counted::now`]) or, in the
/// tests, any.
#[derive(Clone, Copy, Debug)]
pub struct Counted(Lang);

impl Counted {
    /// The texts in the interface's language.
    pub fn now() -> Self {
        Self(locale::current())
    }

    /// `n` grouped as the language writes it.
    fn n(self, n: u64) -> String {
        format::count_in(self.0, n)
    }

    /// The panel's status line: of the sizes, files and folders shown, how much is selected -
    /// each pair `(selected, all)`.
    pub fn panel_status(
        self,
        (selected_size, size): (String, String),
        (selected_files, files): (u64, u64),
        (selected_dirs, dirs): (u64, u64),
    ) -> String {
        let lang = self.0;
        let files = t!(in lang; "panel.status.files",
            count = self.n(files),
            selected = self.n(selected_files),
        );
        let dirs = t!(in lang; "panel.status.dirs",
            count = self.n(dirs),
            selected = self.n(selected_dirs),
        );
        t!(in lang; "panel.status",
            selected_size = selected_size,
            size = size,
            files = files,
            dirs = dirs,
        )
    }

    /// "3 files, 1 folder" of the properties dialog.
    pub fn files_and_folders(self, files: u64, dirs: u64) -> String {
        let lang = self.0;
        let files = t!(in lang; "props.files_and_folders.files", count = self.n(files));
        let dirs = t!(in lang; "props.files_and_folders.dirs", count = self.n(dirs));
        t!(in lang; "props.files_and_folders", files = files, dirs = dirs)
    }

    /// What the disk usage window has counted so far.
    pub fn disk_usage_total(self, size: String, files: u64, folders: u64) -> String {
        let lang = self.0;
        let files = t!(in lang; "disk_usage.total.files", count = self.n(files));
        let folders = t!(in lang; "disk_usage.total.folders", count = self.n(folders));
        t!(in lang; "disk_usage.total", size = size, files = files, folders = folders)
    }

    /// What the quick view shows of a folder.
    pub fn quick_dir(self, files: u64, dirs: u64, size: String) -> String {
        let lang = self.0;
        let files = t!(in lang; "viewer.quick_dir.files", count = self.n(files));
        let dirs = t!(in lang; "viewer.quick_dir.dirs", count = self.n(dirs));
        t!(in lang; "viewer.quick_dir", files = files, dirs = dirs, size = size)
    }

    /// The question before a folder of the connection manager is deleted with what it holds.
    pub fn delete_folder(self, name: &str, folders: u64, connections: u64) -> String {
        let lang = self.0;
        let folders = t!(in lang; "connections.delete_folder.folders", count = self.n(folders));
        let connections =
            t!(in lang; "connections.delete_folder.connections", count = self.n(connections));
        t!(in lang; "connections.delete_folder",
            name = name,
            folders = folders,
            connections = connections,
        )
    }

    /// A job's files and bytes done of all.
    pub fn progress_counts(
        self,
        (files_done, files): (u64, u64),
        (bytes_done, bytes): (String, String),
    ) -> String {
        let lang = self.0;
        let files = t!(in lang; "ops.progress_counts.files",
            count = self.n(files),
            done = self.n(files_done),
        );
        t!(in lang; "ops.progress_counts", files = files, bytes_done = bytes_done, bytes = bytes)
    }

    /// What an inventory's preview or sync did to the connections.
    pub fn inventory_report(
        self,
        created: u64,
        updated: u64,
        unchanged: u64,
        missing: u64,
        skipped: u64,
    ) -> String {
        let lang = self.0;
        t!(in lang; "inventory.report",
            created = t!(in lang; "inventory.report.created", count = self.n(created)),
            updated = t!(in lang; "inventory.report.updated", count = self.n(updated)),
            unchanged = t!(in lang; "inventory.report.unchanged", count = self.n(unchanged)),
            missing = t!(in lang; "inventory.report.missing", count = self.n(missing)),
            skipped = t!(in lang; "inventory.report.skipped", count = self.n(skipped)),
        )
    }

    /// The title of a checksum file's check: its lines by what they found, `[ok, mismatch,
    /// missing, unreadable, invalid]`.
    pub fn checksum_verified(self, file: &str, lines: [u64; 5]) -> String {
        let lang = self.0;
        let [ok, mismatch, missing, unreadable, invalid] = lines;
        t!(in lang; "checksum.result.verified",
            file = file,
            ok = t!(in lang; "checksum.result.verified.ok", count = self.n(ok)),
            mismatch = t!(in lang; "checksum.result.verified.mismatch", count = self.n(mismatch)),
            missing = t!(in lang; "checksum.result.verified.missing", count = self.n(missing)),
            unreadable =
                t!(in lang; "checksum.result.verified.unreadable", count = self.n(unreadable)),
            invalid = t!(in lang; "checksum.result.verified.invalid", count = self.n(invalid)),
        )
    }

    /// The note a host key import leaves: `[added, unchanged, conflicts, skipped]`.
    pub fn host_keys_imported(self, path: &str, keys: [u64; 4]) -> String {
        let lang = self.0;
        let [added, unchanged, conflicts, skipped] = keys;
        t!(in lang; "settings.host_keys.imported",
            path = path,
            added = t!(in lang; "settings.host_keys.imported.added", count = self.n(added)),
            unchanged =
                t!(in lang; "settings.host_keys.imported.unchanged", count = self.n(unchanged)),
            conflicts =
                t!(in lang; "settings.host_keys.imported.conflicts", count = self.n(conflicts)),
            skipped = t!(in lang; "settings.host_keys.imported.skipped", count = self.n(skipped)),
        )
    }

    /// What a selection read from a file or the clipboard found.
    pub fn selection_loaded(self, selected: u64, missing: u64) -> String {
        let lang = self.0;
        let selected = t!(in lang; "selection.loaded.selected", count = self.n(selected));
        let missing = t!(in lang; "selection.loaded.missing", count = self.n(missing));
        t!(in lang; "selection.loaded", selected = selected, missing = missing)
    }

    /// An archive test's result: `[passed, damaged, encrypted]` entries.
    pub fn archive_test(self, name: &str, state: &str, entries: [u64; 3]) -> String {
        let lang = self.0;
        let [passed, damaged, encrypted] = entries;
        t!(in lang; "archive.test.result",
            name = name,
            state = state,
            passed = t!(in lang; "archive.test.result.passed", count = self.n(passed)),
            damaged = t!(in lang; "archive.test.result.damaged", count = self.n(damaged)),
            encrypted = t!(in lang; "archive.test.result.encrypted", count = self.n(encrypted)),
        )
    }

    /// What a synchronisation's comparison found: `[equal, different, left_only, right_only]`.
    pub fn sync_found(self, files: [u64; 4]) -> String {
        let lang = self.0;
        let [equal, different, left_only, right_only] = files;
        t!(in lang; "sync.found",
            equal = t!(in lang; "sync.found.equal", count = self.n(equal)),
            different = t!(in lang; "sync.found.different", count = self.n(different)),
            left_only = t!(in lang; "sync.found.left_only", count = self.n(left_only)),
            right_only = t!(in lang; "sync.found.right_only", count = self.n(right_only)),
        )
    }

    /// A transfer list loaded with entries that could not be.
    pub fn loaded_unloadable(self, jobs: u64, unloadable: u64) -> String {
        let lang = self.0;
        let unloadable =
            t!(in lang; "queue.note.loaded_unloadable.unloadable", count = self.n(unloadable));
        t!(in lang; "queue.note.loaded_unloadable", count = self.n(jobs), unloadable = unloadable)
    }

    /// Attributes set on some entries and not on others.
    pub fn attributes_partly(self, changed: u64, failed: u64) -> String {
        let lang = self.0;
        let failed = t!(in lang; "props.attributes_partly.failed", count = self.n(failed));
        t!(in lang; "props.attributes_partly", count = self.n(changed), failed = failed)
    }

    /// How many of the multi-rename tool's entries get a new name.
    pub fn rename_changes(self, changes: u64, total: u64) -> String {
        let lang = self.0;
        let total = t!(in lang; "multi_rename.changes.total", count = self.n(total));
        t!(in lang; "multi_rename.changes", count = self.n(changes), total = total)
    }

    /// The conflicts among the multi-rename tool's new names.
    pub fn rename_conflicts(self, conflicts: u64, changes: u64) -> String {
        let lang = self.0;
        let changes = t!(in lang; "multi_rename.conflicts.changes", count = self.n(changes));
        t!(in lang; "multi_rename.conflicts", count = self.n(conflicts), changes = changes)
    }

    /// A running search: what it found among the entries it checked, and where it is.
    pub fn search_running(self, found: u64, entries: u64, dir: String) -> String {
        let lang = self.0;
        let entries = t!(in lang; "search.running.entries", count = self.n(entries));
        t!(in lang; "search.running", count = self.n(found), entries = entries, dir = dir)
    }
}

#[cfg(test)]
#[path = "counted_tests.rs"]
mod tests;
