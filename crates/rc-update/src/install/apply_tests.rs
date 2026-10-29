//! The updater's run on a fixture installation: what it changes and what it leaves, without a
//! window and, but for the Unix candidate that ends at once, without starting a program.

use std::fs;

use super::*;
use crate::install::fixture::Fixture;
#[cfg(unix)]
use crate::install::fixture::{NEW, tar_archive};
#[cfg(unix)]
use crate::sign::hex_sha256;

fn no_wait() -> Duration {
    Duration::ZERO
}

fn plenty(_: &Path) -> Option<u64> {
    Some(u64::MAX)
}

fn run_fixture(fixture: &mut Fixture) -> i32 {
    fixture.journal.write().expect("journal");
    run_with(&Journal::path(&fixture.data), &plenty, no_wait()).expect("run")
}

/// "Install when quitting": the switch is made, the new version waits for the next start.
#[test]
fn an_update_left_to_the_next_start_switches_and_starts_nothing() {
    for mut fixture in [Fixture::zip(), Fixture::tar()] {
        fixture.journal.plan.start_new = false;
        assert_eq!(run_fixture(&mut fixture), 0);
        let stored = fixture.stored();
        assert_eq!(stored.phase, Phase::Switched);
        assert!(!stored.new_started);
        fixture.assert_new();
    }
}

/// While a window still runs, nothing changes and nothing starts.
#[test]
fn a_window_that_does_not_end_leaves_everything_as_it_was() {
    let mut fixture = Fixture::zip();
    let _window = InstanceLock::share(&fixture.data)
        .expect("lock")
        .expect("free");
    assert_eq!(run_fixture(&mut fixture), 1);
    let stored = fixture.stored();
    assert_eq!(stored.phase, Phase::Failed);
    assert_eq!(stored.reason.as_deref(), Some("update.app_did_not_end"));
    fixture.assert_old();
}

/// T-SWAP: an artifact exchanged after the check is never unpacked.
#[test]
fn an_artifact_swapped_after_the_check_is_not_unpacked() {
    let mut fixture = Fixture::zip();
    fixture.journal.plan.start_new = false;
    let mut bytes = fs::read(&fixture.journal.plan.artifact).expect("artifact");
    bytes[10] ^= 0xff;
    fs::write(&fixture.journal.plan.artifact, bytes).expect("swap");
    assert_eq!(run_fixture(&mut fixture), 1);
    let stored = fixture.stored();
    assert_eq!(stored.reason.as_deref(), Some("update.digest_mismatch"));
    fixture.assert_old();
    assert!(!fixture.journal.staged_dir().exists());
}

#[test]
fn a_program_folder_without_room_is_refused_before_anything_moves() {
    let mut fixture = Fixture::zip();
    fixture.journal.plan.start_new = false;
    fixture.journal.write().expect("journal");
    let tight = |_: &Path| Some(1);
    let code = run_with(&Journal::path(&fixture.data), &tight, no_wait()).expect("run");
    assert_eq!(code, 1);
    assert_eq!(
        fixture.stored().reason.as_deref(),
        Some("update.not_enough_space")
    );
    fixture.assert_old();
}

/// A candidate that ends right after its start is taken back with the database copy.
#[cfg(unix)]
#[test]
fn a_candidate_that_ends_at_once_is_rolled_back_with_its_database() {
    let mut fixture = Fixture::tar();
    let artifact = fixture.journal.plan.artifact.clone();
    let folder = "rcommander-2.0.0-linux-x64";
    tar_archive(
        &artifact,
        &[
            (format!("{folder}/rcommander"), "#!/bin/sh\nexit 3\n"),
            (format!("{folder}/VERSION.txt"), NEW),
        ],
    );
    let bytes = fs::read(&artifact).expect("artifact");
    fixture.journal.plan.sha256 = hex_sha256(&bytes);
    fixture.journal.plan.size = bytes.len() as u64;
    fixture.journal.plan.confirm_timeout_secs = 30;
    // The new version migrated the database before it ended.
    fs::write(
        fixture.data.join("rcommander.db"),
        "migrated by the new version",
    )
    .expect("db");
    assert_eq!(run_fixture(&mut fixture), 2);
    let stored = fixture.stored();
    assert_eq!(stored.phase, Phase::RolledBack);
    assert_eq!(stored.reason.as_deref(), Some("update.new_version_exited"));
    fixture.assert_old();
    assert_eq!(
        fs::read_to_string(fixture.data.join("rcommander.db")).expect("db"),
        "database before the update"
    );
}
