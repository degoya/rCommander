use rc_vfs::{Attributes, Entry, EntryKind};

use super::*;

fn file(name: &str, mode: Option<u32>) -> Entry {
    Entry {
        name: name.to_owned(),
        kind: EntryKind::File,
        size: 1,
        modified: None,
        created: None,
        accessed: None,
        attributes: Attributes {
            mode,
            ..Attributes::default()
        },
        hidden: false,
        owner: None,
    }
}

fn programs() -> Vec<String> {
    split_extensions(DEFAULT_PROGRAMS).to_vec()
}

#[test]
fn programs_are_executables_on_unix() {
    let extensions = programs();
    assert!(is_program(&file("setup.EXE", None), &extensions, false));
    assert!(is_program(&file("run.cmd", None), &extensions, false));
    assert!(!is_program(&file("notes.txt", None), &extensions, false));
    // Off Windows a file with an execute bit is a program whatever its name.
    assert!(is_program(
        &file("build.sh", Some(0o755)),
        &extensions,
        true
    ));
    assert!(is_program(&file("tool", Some(0o100)), &extensions, true));
    assert!(!is_program(&file("tool", Some(0o644)), &extensions, true));
    // On Windows the bit does not count.
    assert!(!is_program(
        &file("build.sh", Some(0o755)),
        &extensions,
        false
    ));
}

#[test]
fn a_mask_judges_files_and_lets_folders_through() {
    let Some(matcher) = FileFilter::Mask("*.txt;*.md|read*".into()).matcher() else {
        panic!("a mask filters");
    };
    assert!(matcher.shows(&file("a.txt", None)));
    assert!(!matcher.shows(&file("readme.md", None)));
    assert!(!matcher.shows(&file("a.rs", None)));
    assert!(
        matcher.shows(&file("sub\\a.txt", None)),
        "a flat view's name"
    );
    let folder = Entry {
        kind: EntryKind::Dir,
        ..file("src", None)
    };
    assert!(matcher.shows(&folder));
    assert!(FileFilter::All.matcher().is_none());
}

#[test]
fn a_filter_is_checked_and_stored() {
    assert_eq!(
        FileFilter::Mask("  ".into()).checked().ok(),
        Some(FileFilter::All)
    );
    assert_eq!(
        FileFilter::Mask(" *.txt ".into()).checked().ok(),
        Some(FileFilter::Mask("*.txt".into()))
    );
    assert!(
        FileFilter::Mask("x".repeat(MAX_FILTER_MASK + 1))
            .checked()
            .is_err()
    );
    assert!(FileFilter::Mask("a\u{1}".into()).checked().is_err());
    for filter in [
        FileFilter::All,
        FileFilter::Programs,
        FileFilter::Mask("*.rs".into()),
    ] {
        assert_eq!(FileFilter::from_stored(&filter.stored()), filter);
    }
    assert_eq!(FileFilter::All.stored(), "null");
    assert_eq!(FileFilter::from_stored("garbage"), FileFilter::All);
    let json = serde_json::to_value(FileFilter::Mask("*.rs".into())).expect("json");
    assert_eq!(json, serde_json::json!({"kind": "mask", "mask": "*.rs"}));
    let json = serde_json::to_value(FileFilter::Programs).expect("json");
    assert_eq!(json, serde_json::json!({"kind": "programs"}));
}

#[test]
fn the_programs_follow_the_setting() {
    set_programs("ps1 .PY");
    assert_eq!(&*super::programs(), ["ps1".to_owned(), "py".to_owned()]);
    set_programs(DEFAULT_PROGRAMS);
}
