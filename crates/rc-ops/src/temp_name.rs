//! The names rCommander gives a file beside its target for a while, in one place (RC-080-48): a
//! copy's part file `<name>.rcpart`, a packed archive's `.<name>.<job>.rcpart`, a case-only
//! rename's `.rc-tmp-<process>-<time>-<n>` and a multi-rename's `.rc-rename-<tag>-<n>` - the last
//! made by `rc_rename`, which this crate builds on, from its [`rc_rename::TEMP_PREFIX`].
//! [`is_temp_name`] knows every one of them, for whatever sweeps or hides what a run left.

use std::path::{Path, PathBuf};

/// What a part file adds to its target's name.
pub(crate) const PART_SUFFIX: &str = ".rcpart";

/// How a case-only rename's temporary name starts.
pub(crate) const CASE_PREFIX: &str = ".rc-tmp-";

/// The part file of `dest`: `<name>.rcpart` beside it.
pub(crate) fn part_path(dest: &Path) -> PathBuf {
    let mut name = dest.file_name().unwrap_or_default().to_os_string();
    name.push(PART_SUFFIX);
    dest.with_file_name(name)
}

/// The temporary file the archive `dest` of job `job` is written to: beside it, hidden.
pub(crate) fn pack_path(dest: &Path, job: u64) -> PathBuf {
    let name = dest
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    dest.with_file_name(format!(".{name}.{job}{PART_SUFFIX}"))
}

/// The `n`-th candidate in `dir` for a case-only rename's temporary name, `nanos` the run's
/// time: `.rc-tmp-<process>-<time>-<n>`.
pub(crate) fn case_path(dir: &Path, nanos: u32, n: u32) -> PathBuf {
    dir.join(format!(
        "{CASE_PREFIX}{:x}-{nanos:x}-{n}",
        std::process::id()
    ))
}

/// Whether the file name `name` is one of rCommander's temporary names.
#[must_use]
pub fn is_temp_name(name: &str) -> bool {
    name.ends_with(PART_SUFFIX)
        || name.starts_with(CASE_PREFIX)
        || name.starts_with(rc_rename::TEMP_PREFIX)
}

#[cfg(test)]
#[path = "temp_name_tests.rs"]
mod tests;
