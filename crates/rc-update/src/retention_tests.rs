use std::time::{Duration, SystemTime};

use super::*;
use crate::install::fixture::write;

const DAY: Duration = Duration::from_secs(24 * 60 * 60);

struct Folders {
    _root: tempfile::TempDir,
    data: PathBuf,
    install: PathBuf,
}

/// Two copies (the newer one an hour younger), a download, a partial one and foreign files.
fn folders() -> Folders {
    let root = tempfile::tempdir().expect("tempdir");
    let data = root.path().join("data");
    let install = root.path().join("install");
    let pre = update_dir(&data).join(PRE_UPDATE_DIR);
    let download = update_dir(&data).join(DOWNLOAD_DIR);
    write(
        &pre.join("rcommander-1.0.0-20261001T000000Z.db"),
        "older copy",
    );
    write(
        &pre.join("rcommander-1.1.0-20261002T000000Z.db"),
        "newer copy",
    );
    let older = fs::File::options()
        .write(true)
        .open(pre.join("rcommander-1.0.0-20261001T000000Z.db"))
        .expect("open");
    older
        .set_modified(SystemTime::now() - Duration::from_secs(3600))
        .expect("age");
    write(&pre.join("notes.txt"), "the person's own");
    write(
        &download.join("rcommander-1.1.0-windows-x64.zip"),
        "archive",
    );
    write(
        &download.join("rcommander-1.2.0-windows-x64.zip.partial"),
        "half",
    );
    write(&download.join("other.zip"), "foreign");
    Folders {
        _root: root,
        data,
        install,
    }
}

fn names(folder: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(folder)
        .expect("list")
        .map(|entry| {
            entry
                .expect("entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    names.sort();
    names
}

fn policy(proven: bool, retention_days: u32, now: SystemTime) -> Policy {
    Policy {
        proven,
        retention_days,
        now,
    }
}

#[test]
fn a_proven_update_keeps_only_the_newest_copy_and_empties_the_downloads() {
    let folders = folders();
    let report = sweep(
        &folders.data,
        &folders.install,
        policy(true, 14, SystemTime::now()),
    );
    assert_eq!(report.removed.len(), 2);
    let update = update_dir(&folders.data);
    assert_eq!(
        names(&update.join(PRE_UPDATE_DIR)),
        vec!["notes.txt", "rcommander-1.1.0-20261002T000000Z.db"]
    );
    assert_eq!(
        names(&update.join(DOWNLOAD_DIR)),
        vec!["other.zip", "rcommander-1.2.0-windows-x64.zip.partial"]
    );
}

#[test]
fn after_the_retention_the_newest_copy_goes_too_and_zero_keeps_it() {
    let folders = folders();
    let later = SystemTime::now() + 15 * DAY;
    let _ = sweep(&folders.data, &folders.install, policy(true, 0, later));
    let pre = update_dir(&folders.data).join(PRE_UPDATE_DIR);
    assert_eq!(
        names(&pre),
        vec!["notes.txt", "rcommander-1.1.0-20261002T000000Z.db"],
        "0 keeps it for good"
    );
    let _ = sweep(&folders.data, &folders.install, policy(true, 14, later));
    assert_eq!(names(&pre), vec!["notes.txt"]);
}

#[test]
fn an_unproven_update_keeps_everything() {
    let folders = folders();
    let report = sweep(
        &folders.data,
        &folders.install,
        policy(false, 1, SystemTime::now() + 30 * DAY),
    );
    assert_eq!(report, Report::default());
    assert_eq!(
        names(&update_dir(&folders.data).join(PRE_UPDATE_DIR)).len(),
        3
    );
    assert_eq!(
        names(&update_dir(&folders.data).join(DOWNLOAD_DIR)).len(),
        3
    );
}

#[test]
fn the_leftovers_are_counted_without_foreign_files() {
    let folders = folders();
    let named = "older copy".len() + "newer copy".len() + "archive".len();
    assert_eq!(
        leftover_bytes(&folders.data, &folders.install),
        named as u64
    );
}
