//! Sorting the system's notices, and which folder they concern.

use std::path::{Path, PathBuf};

use notify::{
    Event, EventKind,
    event::{
        AccessKind, CreateKind, DataChange, Flag, MetadataKind, ModifyKind, RemoveKind, RenameMode,
    },
};

use super::{Change, Signal, brings_new, change_of, concerns, signal_of};

#[test]
fn deleted_and_renamed_entries_are_structure_and_new_ones_a_creation() {
    assert_eq!(
        change_of(&EventKind::Create(CreateKind::File)),
        Some(Change::Created)
    );
    for kind in [
        EventKind::Remove(RemoveKind::Folder),
        EventKind::Modify(ModifyKind::Name(RenameMode::Both)),
        EventKind::Any,
    ] {
        assert_eq!(change_of(&kind), Some(Change::Structure), "{kind:?}");
    }
}

#[test]
fn size_time_and_attributes_are_content_and_an_access_nothing() {
    for kind in [
        EventKind::Modify(ModifyKind::Data(DataChange::Size)),
        EventKind::Modify(ModifyKind::Metadata(MetadataKind::WriteTime)),
        EventKind::Modify(ModifyKind::Any),
    ] {
        assert_eq!(change_of(&kind), Some(Change::Content), "{kind:?}");
    }
    assert_eq!(change_of(&EventKind::Access(AccessKind::Any)), None);
    assert_eq!(change_of(&EventKind::Other), None);
}

#[test]
fn a_lost_notice_or_an_error_is_a_rescan() {
    let lost = Event::new(EventKind::Other).set_flag(Flag::Rescan);
    assert!(matches!(signal_of(Ok(lost)), Some(Signal::Rescan)));
    let error = notify::Error::generic("the queue overflowed");
    assert!(matches!(signal_of(Err(error)), Some(Signal::Rescan)));
    let created = Event::new(EventKind::Create(CreateKind::File)).add_path(PathBuf::from("/w/a"));
    let Some(Signal::Changed { paths, change }) = signal_of(Ok(created)) else {
        panic!("a change");
    };
    assert_eq!(change, Change::Created);
    assert_eq!(paths, [PathBuf::from("/w/a")]);
}

#[test]
fn a_notice_concerns_the_folder_and_its_entries_only() {
    let watched = Path::new("/w/docs");
    assert!(concerns(watched, Path::new("/w/docs/a.txt")));
    assert!(concerns(watched, Path::new("/w/docs")));
    assert!(!concerns(watched, Path::new("/w/docs/sub/a.txt")));
    assert!(!concerns(watched, Path::new("/w/other.txt")));
}

#[test]
fn a_creation_of_a_listed_name_brings_nothing_new() {
    let watched = Path::new("/w/docs");
    let listed = |name: &str| name == "a.txt";
    // FSEvents: a write to a file created a while ago still carries the creation.
    assert!(!brings_new(watched, Path::new("/w/docs/a.txt"), listed));
    assert!(brings_new(watched, Path::new("/w/docs/b.txt"), listed));
    assert!(brings_new(watched, watched, listed));
}
