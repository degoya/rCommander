use std::{
    path::PathBuf,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use rc_vfs::{Attributes, Entry, EntryKind, TargetKind, WindowsFlags};

use super::{Filters, KindFilter};

const DAY: u64 = 86_400;

fn now() -> SystemTime {
    UNIX_EPOCH + Duration::from_secs(1_000 * DAY)
}

fn file(name: &str, size: u64, age_days: u64) -> Entry {
    Entry {
        name: name.to_owned(),
        kind: EntryKind::File,
        size,
        modified: Some(now() - Duration::from_secs(age_days * DAY)),
        created: None,
        attributes: Attributes::default(),
        hidden: false,
        owner: None,
    }
}

fn dir(name: &str) -> Entry {
    Entry {
        kind: EntryKind::Dir,
        ..file(name, 0, 0)
    }
}

#[test]
fn no_filter_admits_everything() {
    let filters = Filters::default();
    assert!(filters.admits(&file("a", 1, 1), now()));
    assert!(filters.admits(&dir("d"), now()));
}

#[test]
fn kind_counts_a_link_as_its_target() {
    let link = |target_kind| Entry {
        kind: EntryKind::Symlink {
            target: PathBuf::from("t"),
            target_kind,
        },
        ..file("l", 0, 0)
    };
    let files = Filters {
        kind: KindFilter::Files,
        ..Filters::default()
    };
    let dirs = Filters {
        kind: KindFilter::Dirs,
        ..Filters::default()
    };
    assert!(files.admits(&file("a", 1, 1), now()));
    assert!(!files.admits(&dir("d"), now()));
    assert!(dirs.admits(&dir("d"), now()));
    assert!(files.admits(&link(Some(TargetKind::File)), now()));
    assert!(dirs.admits(&link(Some(TargetKind::Dir)), now()));
    assert!(!files.admits(&link(None), now()));
}

#[test]
fn size_bounds_are_inclusive_and_leave_directories_out() {
    let filters = Filters {
        min_size: Some(10),
        max_size: Some(20),
        ..Filters::default()
    };
    assert!(!filters.admits(&file("a", 9, 0), now()));
    assert!(filters.admits(&file("a", 10, 0), now()));
    assert!(filters.admits(&file("a", 20, 0), now()));
    assert!(!filters.admits(&file("a", 21, 0), now()));
    assert!(!filters.admits(&dir("d"), now()));
}

#[test]
fn dates_between_and_not_older_than() {
    let secs = |days_ago: u64| 1_000 * DAY - days_ago * DAY;
    let between = Filters {
        modified_after: Some(secs(10)),
        modified_before: Some(secs(5)),
        ..Filters::default()
    };
    assert!(!between.admits(&file("a", 0, 11), now()));
    assert!(between.admits(&file("a", 0, 10), now()));
    assert!(between.admits(&file("a", 0, 5), now()));
    assert!(!between.admits(&file("a", 0, 4), now()));
    let recent = Filters {
        max_age_secs: Some(3 * DAY),
        ..Filters::default()
    };
    assert!(recent.admits(&file("a", 0, 2), now()));
    assert!(!recent.admits(&file("a", 0, 4), now()));
    let unknown = Entry {
        modified: None,
        ..file("a", 0, 0)
    };
    assert!(
        !recent.admits(&unknown, now()),
        "no date meets no date condition"
    );
}

#[test]
fn attributes_are_three_state() {
    let mut flagged = file("a", 0, 0);
    flagged.hidden = true;
    flagged.attributes = Attributes {
        mode: Some(0o755),
        readonly: true,
        windows: Some(WindowsFlags {
            hidden: true,
            system: true,
            archive: false,
        }),
    };
    let plain = file("b", 0, 0);
    let want = |filters: Filters| {
        (
            filters.admits(&flagged, now()),
            filters.admits(&plain, now()),
        )
    };
    let hidden = Filters {
        hidden: Some(true),
        ..Filters::default()
    };
    assert_eq!(want(hidden), (true, false));
    let writable = Filters {
        readonly: Some(false),
        ..Filters::default()
    };
    assert_eq!(want(writable), (false, true));
    let system = Filters {
        system: Some(true),
        ..Filters::default()
    };
    assert_eq!(want(system), (true, false));
    let archive = Filters {
        archive: Some(true),
        ..Filters::default()
    };
    assert_eq!(want(archive), (false, false));
    let executable = Filters {
        mode_bits: Some(0o111),
        ..Filters::default()
    };
    assert_eq!(want(executable), (true, false));
}

#[test]
fn combined_filters_give_the_intersection() {
    let filters = Filters {
        kind: KindFilter::Files,
        min_size: Some(100),
        max_age_secs: Some(7 * DAY),
        readonly: Some(false),
        ..Filters::default()
    };
    let mut readonly = file("ro", 500, 1);
    readonly.attributes.readonly = true;
    let candidates = [
        file("hit", 500, 1),
        file("small", 50, 1),
        file("old", 500, 30),
        readonly,
        dir("dir"),
    ];
    let admitted: Vec<&str> = candidates
        .iter()
        .filter(|entry| filters.admits(entry, now()))
        .map(|entry| entry.name.as_str())
        .collect();
    assert_eq!(admitted, ["hit"]);
}
