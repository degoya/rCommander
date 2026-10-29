//! T-JOURNAL: a journal that reaches outside its installation is laid aside untouched and acted
//! on by nobody.

use std::fs;

use super::*;
use crate::install::fixture::Fixture;

fn rejected(fixture: &Fixture) -> PathBuf {
    update_dir(&fixture.data).join(REJECTED_FILE)
}

#[test]
fn a_journal_pointing_out_of_its_folders_is_laid_aside_and_nothing_happens() {
    let cases: [fn(&mut Journal); 6] = [
        |journal| journal.plan.executable = "../evil.exe".to_owned(),
        |journal| journal.plan.data_dir = PathBuf::from("/elsewhere"),
        |journal| journal.plan.database = PathBuf::from("/elsewhere/rcommander.db"),
        |journal| journal.plan.database_copy = Some(PathBuf::from("/tmp/copy.db")),
        |journal| journal.entries = vec!["..".to_owned()],
        |journal| journal.replaced = vec!["notes.txt".to_owned()],
    ];
    for (index, change) in cases.into_iter().enumerate() {
        let mut fixture = Fixture::zip();
        let sound = fixture.journal.clone();
        change(&mut fixture.journal);
        let bytes = serde_json::to_vec_pretty(&fixture.journal).expect("encode");
        fs::write(Journal::path(&fixture.data), &bytes).expect("write");
        assert_eq!(
            Journal::read(&fixture.data).expect("read"),
            None,
            "case {index}"
        );
        assert!(!Journal::path(&fixture.data).exists(), "case {index}");
        assert_eq!(
            fs::read(rejected(&fixture)).expect("aside"),
            bytes,
            "case {index}"
        );
        let recovery = recover::recover_at_start(
            &fixture.data,
            &fixture.install.join("rcommander.exe"),
            "1.0.0",
        )
        .expect("recover");
        assert_eq!(recovery, recover::Recovery::Continue);
        // The old installation, judged by the journal as it was before the change.
        fixture.journal = sound;
        fixture.assert_old();
    }
}

#[test]
fn a_sound_journal_reads_back_as_written() {
    let fixture = Fixture::zip();
    assert_eq!(fixture.stored(), fixture.journal);
    assert!(!rejected(&fixture).exists());
}

#[test]
fn a_macos_bundle_program_is_a_path_inside_its_entry() {
    let mut fixture = Fixture::zip();
    fixture.journal.plan.executable = "rCommander.app/Contents/MacOS/rcommander".to_owned();
    fixture.journal.entries = vec!["rCommander.app".to_owned()];
    assert!(fixture.journal.check(&fixture.data).is_ok());
    assert_eq!(fixture.journal.plan.program_entry(), "rCommander.app");
    fixture.journal.entries = vec!["Other.app".to_owned()];
    assert!(fixture.journal.check(&fixture.data).is_err());
}

#[test]
fn preflight_refuses_a_folder_without_room() {
    let folder = tempfile::tempdir().expect("tempdir");
    assert!(preflight(folder.path(), 100, Some(200)).is_ok());
    assert!(preflight(folder.path(), 100, None).is_ok());
    let error = preflight(folder.path(), 100, Some(199)).expect_err("no room");
    assert_eq!(error.code, "update.not_enough_space");
    let missing = folder.path().join("missing");
    let error = preflight(&missing, 100, None).expect_err("not writable");
    assert_eq!(error.code, "update.install_dir_not_writable");
}

#[test]
fn a_plan_names_portable_installs_and_plain_versions_only() {
    let mut fixture = Fixture::zip();
    fixture.journal.plan.kind = crate::install_kind::InstallKind::Unknown;
    assert_eq!(
        fixture.journal.plan.validate().map_err(|error| error.code),
        Err("update.plan_invalid")
    );
    let mut fixture = Fixture::zip();
    fixture.journal.plan.target_version = "../2.0.0".to_owned();
    assert!(fixture.journal.plan.validate().is_err());
}
