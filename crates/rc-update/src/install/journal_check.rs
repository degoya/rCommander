//! What a journal may name before anything acts on it (T-JOURNAL, `docs/security/updates.md`).
//!
//! Copied and adapted from rDownloader's `crates/rd-update/src/install/journal_check.rs` at commit
//! 26c8592a6 (RC-100-06): the data directory apart from the program folder, the program a
//! relative path for the macOS bundle, no installer.

use std::fs;
use std::path::{Component, Path};

use super::{InstallError, Journal, PRE_UPDATE_DIR, portable, update_dir};

impl Journal {
    /// Refuses a journal that reaches outside the installation it belongs to: whatever a start
    /// does with it moves program files, puts a database in place and starts a program. On top
    /// of [`super::Plan::validate`]: the journal lies in the data directory it names, the database
    /// in it, its copy in `update/pre-update`; the program, the entries and the replaced ones are
    /// plain names in the program folder.
    ///
    /// # Errors
    ///
    /// `update.journal_invalid` with what is wrong.
    pub fn check(&self, data: &Path) -> Result<(), InstallError> {
        let invalid = |detail: String| Err(InstallError::new("update.journal_invalid", detail));
        if let Err(error) = self.plan.validate() {
            return invalid(error.detail);
        }
        let plan = &self.plan;
        let canonical = |path: &Path| fs::canonicalize(path).unwrap_or_else(|_| path.to_owned());
        if canonical(&plan.data_dir) != canonical(data) {
            return invalid(format!(
                "the journal in {} names the data directory {}",
                data.display(),
                plan.data_dir.display()
            ));
        }
        let database_name = plan
            .database
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default();
        if plan.database.parent() != Some(plan.data_dir.as_path()) || !is_name(database_name) {
            return invalid(format!(
                "the database {} is not in the data directory",
                plan.database.display()
            ));
        }
        if let Some(copy) = &plan.database_copy
            && !is_below(copy, &update_dir(&plan.data_dir).join(PRE_UPDATE_DIR))
        {
            return invalid(format!(
                "the database copy {} is not in update/{PRE_UPDATE_DIR}",
                copy.display()
            ));
        }
        if plan.executable.is_empty() || !plan.executable.split('/').all(is_name) {
            return invalid(format!(
                "{:?} is not a program in the program folder",
                plan.executable
            ));
        }
        if let Some(name) = self
            .entries
            .iter()
            .chain(&self.replaced)
            .find(|name| !is_name(name))
        {
            return invalid(format!("{name:?} is not a name in the program folder"));
        }
        if let Some(name) = self.entries.iter().find(|name| {
            portable::KEPT
                .iter()
                .any(|kept| kept.eq_ignore_ascii_case(name))
        }) {
            return invalid(format!("{name:?} is never replaced by an update"));
        }
        if let Some(name) = self
            .replaced
            .iter()
            .find(|name| !self.entries.contains(name))
        {
            return invalid(format!("{name:?} is replaced but no entry of the update"));
        }
        if !self.entries.is_empty()
            && !self
                .entries
                .iter()
                .any(|entry| entry == plan.program_entry())
        {
            return invalid(format!(
                "the program {} is no entry of the update",
                plan.executable
            ));
        }
        Ok(())
    }
}

/// One plain, visible name: a single normal path component that does not start with `.`, so
/// joined to a folder it stays inside it on every platform (`C:x` and `a\b` are two on Windows).
pub(crate) fn is_name(name: &str) -> bool {
    let mut components = Path::new(name).components();
    !name.is_empty()
        && !name.starts_with('.')
        && !name.contains(['/', '\\', ':'])
        && matches!(components.next(), Some(Component::Normal(_)))
        && components.next().is_none()
}

/// Whether `path` lies below `folder`, lexically: no `..`, no second root.
fn is_below(path: &Path, folder: &Path) -> bool {
    path.strip_prefix(folder).is_ok_and(|rest| {
        rest.components().next().is_some()
            && rest
                .components()
                .all(|component| matches!(component, Component::Normal(_)))
    })
}
