//! What the next start does after a stop at each phase: it ends in the whole old or the whole new
//! version, never in between.

use std::fs;

use super::*;
use crate::install::fixture::{Fixture, NEW, OLD};

fn exe(fixture: &Fixture) -> PathBuf {
    fixture.journal.executable()
}

fn stored_phase(fixture: &Fixture) -> (Phase, Option<String>) {
    let stored = fixture.stored();
    (stored.phase, stored.reason)
}

#[test]
fn a_stop_before_the_switch_leaves_the_old_version_and_records_the_interruption() {
    for staged in [false, true] {
        let mut fixture = Fixture::zip();
        if staged {
            portable::stage(&mut fixture.journal).expect("stage");
        }
        let recovery = recover_at_start(&fixture.data, &exe(&fixture), OLD).expect("recover");
        assert_eq!(recovery, Recovery::Continue);
        assert_eq!(
            stored_phase(&fixture),
            (Phase::Failed, Some("update.interrupted".to_owned()))
        );
        fixture.assert_old();
    }
}

#[test]
fn a_stop_during_the_switch_is_taken_back_and_the_newer_program_restarts_as_the_old() {
    let mut fixture = Fixture::zip();
    portable::stage(&mut fixture.journal).expect("stage");
    fixture
        .journal
        .advance(Phase::Switching)
        .expect("switching");
    // Half the switch: the program itself already swapped.
    let previous = fixture.journal.previous_dir();
    fs::create_dir_all(&previous).expect("previous");
    let program = fixture.install.join("rcommander.exe");
    fs::rename(&program, previous.join("rcommander.exe")).expect("aside");
    fs::rename(
        fixture.journal.staged_dir().join("rcommander.exe"),
        &program,
    )
    .expect("place");
    let recovery = recover_at_start(&fixture.data, &exe(&fixture), NEW).expect("recover");
    assert_eq!(recovery, Recovery::Restart(exe(&fixture)));
    assert_eq!(
        stored_phase(&fixture),
        (Phase::RolledBack, Some("update.interrupted".to_owned()))
    );
    fixture.assert_old();
}

/// Switched and never proven: the first start of the new version may prove it; a second
/// unproven one takes it back with the database copy.
#[test]
fn a_second_unproven_start_of_the_new_version_takes_it_back() {
    let mut fixture = Fixture::zip();
    portable::stage(&mut fixture.journal).expect("stage");
    portable::switch(&mut fixture.journal).expect("switch");
    fs::write(fixture.data.join("rcommander.db"), "migrated").expect("db");
    let first = recover_at_start(&fixture.data, &exe(&fixture), NEW).expect("first");
    assert_eq!(first, Recovery::Continue);
    assert_eq!(fixture.stored().start_attempts, 1);
    fixture.assert_new();
    let second = recover_at_start(&fixture.data, &exe(&fixture), NEW).expect("second");
    assert_eq!(second, Recovery::Restart(exe(&fixture)));
    assert_eq!(
        stored_phase(&fixture),
        (Phase::RolledBack, Some("update.not_confirmed".to_owned()))
    );
    fixture.assert_old();
    assert_eq!(
        fs::read_to_string(fixture.data.join("rcommander.db")).expect("db"),
        "database before the update"
    );
}

#[test]
fn the_first_start_proves_the_update_and_a_later_one_cleans_up_once() {
    let mut fixture = Fixture::zip();
    portable::stage(&mut fixture.journal).expect("stage");
    portable::switch(&mut fixture.journal).expect("switch");
    assert_eq!(
        recover_at_start(&fixture.data, &exe(&fixture), NEW).expect("recover"),
        Recovery::Continue
    );
    assert!(!confirm_started(&fixture.data, OLD).expect("other version"));
    assert!(confirm_started(&fixture.data, NEW).expect("confirm"));
    assert_eq!(fixture.stored().phase, Phase::Verified);
    assert!(update_proven(&fixture.data, NEW));
    assert!(!update_proven(&fixture.data, OLD));
    recover_at_start(&fixture.data, &exe(&fixture), NEW).expect("clean");
    let stored = fixture.stored();
    assert!(stored.cleaned);
    assert!(
        !fixture.journal.previous_dir().exists(),
        "the old version is gone"
    );
    assert_eq!(
        fixture.read("rcommander.exe").as_deref(),
        Some("new program")
    );
}

/// The copy is put back only while its BLAKE3 is the journal's.
#[test]
fn the_database_copy_is_put_back_only_while_its_digest_matches() {
    let mut fixture = Fixture::zip();
    let copy = fixture.journal.plan.database_copy.clone().expect("copy");
    fs::write(&copy, "changed since the backup").expect("tamper");
    fs::write(fixture.data.join("rcommander.db"), "migrated").expect("db");
    fixture.journal.new_started = true;
    assert!(steps::restore_database(&fixture.journal.plan).is_err());
    assert_eq!(
        fs::read_to_string(fixture.data.join("rcommander.db")).expect("db"),
        "migrated"
    );
    let mut fresh = Fixture::zip();
    fresh.journal.new_started = true;
    assert!(steps::restore_database(&fresh.journal.plan).expect("restore"));
    assert!(
        !fresh.data.join("rcommander.db-wal").exists(),
        "the WAL goes too"
    );
}

#[test]
fn nothing_happens_while_the_updater_runs_or_for_another_program() {
    let mut fixture = Fixture::zip();
    portable::stage(&mut fixture.journal).expect("stage");
    let held = crate::install::UpdaterLock::acquire(&fixture.data)
        .expect("lock")
        .expect("free");
    assert_eq!(
        recover_at_start(&fixture.data, &exe(&fixture), OLD).expect("recover"),
        Recovery::Continue
    );
    assert_eq!(fixture.stored().phase, Phase::Staged);
    drop(held);
    let elsewhere = fixture.install.join("elsewhere").join("rcommander.exe");
    assert_eq!(
        recover_at_start(&fixture.data, &elsewhere, OLD).expect("recover"),
        Recovery::Continue
    );
    assert_eq!(fixture.stored().phase, Phase::Staged);
}
