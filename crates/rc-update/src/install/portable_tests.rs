//! The switch and its roll-back from every step: a stop after any rename ends, taken back, in
//! the whole old version, or, finished, in the whole new one.

use super::*;
use crate::install::fixture::Fixture;

#[test]
fn stage_strips_the_version_folder_and_records_the_entries() {
    let mut fixture = Fixture::zip();
    stage(&mut fixture.journal).expect("stage");
    assert_eq!(fixture.journal.phase, Phase::Staged);
    assert_eq!(
        fixture.journal.entries,
        vec!["LICENSE", "README.md", "VERSION.txt", "rcommander.exe"]
    );
    assert_eq!(
        fixture.journal.replaced,
        vec!["README.md", "VERSION.txt", "rcommander.exe"]
    );
    // Nothing live changed, and the archive's `data/` is never an entry.
    fixture.assert_untouched();
    assert_eq!(
        fixture.read("rcommander.exe").as_deref(),
        Some("old program")
    );
}

#[test]
fn a_switch_puts_the_new_version_in_place_and_the_old_one_aside() {
    for mut fixture in [Fixture::zip(), Fixture::tar()] {
        stage(&mut fixture.journal).expect("stage");
        switch(&mut fixture.journal).expect("switch");
        assert_eq!(fixture.journal.phase, Phase::Switched);
        fixture.assert_new();
    }
}

/// A switch stopped after `stop_after` renames: taken back it is the old version, run again it is
/// the new one.
#[test]
fn a_switch_stopped_after_any_rename_is_taken_back_or_finished() {
    let renames = {
        let mut fixture = Fixture::zip();
        stage(&mut fixture.journal).expect("stage");
        fixture.journal.entries.len() + fixture.journal.replaced.len()
    };
    for stop_after in 0..=renames {
        for finish in [false, true] {
            let mut fixture = Fixture::zip();
            stage(&mut fixture.journal).expect("stage");
            fixture
                .journal
                .advance(Phase::Switching)
                .expect("switching");
            partial_switch(&fixture.journal, stop_after);
            if finish {
                switch(&mut fixture.journal).expect("finish");
                fixture.assert_new();
            } else {
                roll_back(&fixture.journal).expect("roll back");
                fixture.assert_old();
            }
        }
    }
}

/// The first `count` renames of a switch, as a stop would leave them.
fn partial_switch(journal: &Journal, count: usize) {
    let install = &journal.plan.install_dir;
    let previous = journal.previous_dir();
    std::fs::create_dir_all(&previous).expect("previous");
    let mut done = 0;
    for name in &journal.entries {
        let live = install.join(name);
        if exists(&live) {
            if done == count {
                return;
            }
            std::fs::rename(&live, previous.join(name)).expect("aside");
            done += 1;
        }
        if done == count {
            return;
        }
        std::fs::rename(journal.staged_dir().join(name), &live).expect("place");
        done += 1;
    }
}

#[test]
fn an_archive_without_the_program_is_refused_and_changes_nothing() {
    let mut fixture = Fixture::zip();
    fixture.journal.plan.executable = "rcommander-other.exe".to_owned();
    let error = stage(&mut fixture.journal).expect_err("no program");
    assert_eq!(
        error.downcast_ref::<InstallError>().map(|error| error.code),
        Some("update.archive_incomplete")
    );
    assert_eq!(
        fixture.read("rcommander.exe").as_deref(),
        Some("old program")
    );
}

#[test]
fn an_unknown_archive_kind_is_refused() {
    let mut fixture = Fixture::zip();
    let renamed = fixture.journal.plan.artifact.with_extension("rar");
    std::fs::rename(&fixture.journal.plan.artifact, &renamed).expect("rename");
    fixture.journal.plan.artifact = renamed;
    let error = stage(&mut fixture.journal).expect_err("unknown");
    assert_eq!(
        error.downcast_ref::<InstallError>().map(|error| error.code),
        Some("update.archive_unknown")
    );
}

/// RC-100-95: a portable copy keeps its data and its marker beside the program; the switch
/// moves neither, even from an archive that carries both.
#[test]
fn the_portable_switch_leaves_data_and_marker() {
    use crate::install::fixture::{write, zip_archive};
    use crate::portable_mode::{DATA_DIR, MARKER};
    let mut fixture = Fixture::zip();
    let data = fixture.install.join(DATA_DIR);
    write(&data.join("rcommander.db"), "the portable database");
    write(&data.join("plugins").join("state.json"), "the plugins");
    write(&fixture.install.join(MARKER), "the marker");
    let folder = "rcommander-2.0.0-windows-x64";
    let files: Vec<(String, &str)> = vec![
        (format!("{folder}/rcommander.exe"), "new program"),
        (
            format!("{folder}/VERSION.txt"),
            crate::install::fixture::NEW,
        ),
        (
            format!("{folder}/{DATA_DIR}/rcommander.db"),
            "a database from the archive",
        ),
        (format!("{folder}/{MARKER}"), "a marker from the archive"),
    ];
    let artifact = fixture.journal.plan.artifact.clone();
    zip_archive(&artifact, &files);
    let bytes = std::fs::read(&artifact).expect("artifact");
    fixture.journal.plan.sha256 = crate::sign::hex_sha256(&bytes);
    fixture.journal.plan.size = bytes.len() as u64;

    stage(&mut fixture.journal).expect("stage");
    assert_eq!(
        fixture.journal.entries,
        vec!["VERSION.txt", "rcommander.exe"]
    );
    switch(&mut fixture.journal).expect("switch");
    assert_eq!(
        fixture.read("rcommander.exe").as_deref(),
        Some("new program")
    );
    let kept = |relative: &str| std::fs::read_to_string(fixture.install.join(relative)).ok();
    assert_eq!(
        kept("data/rcommander.db").as_deref(),
        Some("the portable database")
    );
    assert_eq!(
        kept("data/plugins/state.json").as_deref(),
        Some("the plugins")
    );
    assert_eq!(kept(MARKER).as_deref(), Some("the marker"));
    assert!(!fixture.journal.previous_dir().join(DATA_DIR).exists());
    assert!(!fixture.journal.previous_dir().join(MARKER).exists());
}
