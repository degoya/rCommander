use super::*;
use crate::install::fixture::write;
#[cfg(unix)]
use crate::install::fixture::{locked_folder, unlock};

fn install() -> (tempfile::TempDir, PathBuf) {
    let root = tempfile::tempdir().expect("tempdir");
    let install = root.path().join("install");
    write(&install.join("rcommander"), "program");
    (root, install)
}

#[test]
fn a_leftover_goes_through_the_trash_and_the_trash_goes_with_it() {
    let (_root, install) = install();
    let previous = install.join(".previous");
    write(&previous.join("rcommander"), "old program");
    write(&previous.join("plugins").join("old.rcplug"), "old plugin");
    discard(&previous, &install).expect("discard");
    assert!(!previous.exists());
    assert!(!trash_dir(&install).exists());
    assert!(install.join("rcommander").is_file(), "the program stays");
}

#[test]
fn a_leftover_that_is_not_there_creates_no_trash() {
    let (_root, install) = install();
    discard(&install.join(".previous"), &install).expect("nothing to do");
    assert!(!trash_dir(&install).exists());
    sweep(&install);
}

/// A rest in `.previous/` that cannot be removed yet - on Windows a running `rcommander mcp`,
/// here a folder whose entries may not be unlinked - is moved aside instead of failing, so the
/// name is free for the next update, and goes with a later sweep.
#[cfg(unix)]
#[test]
fn a_rest_that_cannot_be_removed_does_not_hold_up_the_next_update() {
    let (_root, install) = install();
    let previous = install.join(".previous");
    let Some(locked) = locked_folder(&previous) else {
        return;
    };
    discard(&previous, &install).expect("set aside, not failed");
    assert!(!previous.exists(), "the name is free for the next update");
    let batches: Vec<PathBuf> = fs::read_dir(trash_dir(&install))
        .expect("trash")
        .map(|entry| entry.expect("entry").path())
        .collect();
    assert_eq!(batches.len(), 1, "{batches:?}");
    let held = batches[0]
        .join(".previous")
        .join(locked.file_name().expect("name"))
        .join("rcommander-mcp");
    assert!(held.is_file(), "what is in use waits in the trash");
    unlock(held.parent().expect("folder"));
    sweep(&install);
    assert!(!trash_dir(&install).exists());
}
