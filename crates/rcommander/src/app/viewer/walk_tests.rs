use rc_engine::{Entry, EntryKind, ListerFiles};
use rc_vfs::Attributes;

use super::{Walk, next};

fn entries() -> Vec<Entry> {
    ["a.txt", "dir", "b.txt", "c.txt"]
        .into_iter()
        .map(|name| Entry {
            name: name.to_owned(),
            kind: if name == "dir" {
                EntryKind::Dir
            } else {
                EntryKind::File
            },
            size: 1,
            modified: None,
            created: None,
            accessed: None,
            attributes: Attributes::default(),
            hidden: false,
            owner: None,
        })
        .collect()
}

fn step(walk: &Walk, current: &str, forward: bool) -> Option<String> {
    next(&entries(), current, forward, walk).map(|entry| entry.name.clone())
}

#[test]
fn the_panel_s_files_without_its_folders() {
    let walk = Walk::of(ListerFiles::Panel, Vec::new());
    assert_eq!(step(&walk, "a.txt", true).as_deref(), Some("b.txt"));
    assert_eq!(step(&walk, "b.txt", false).as_deref(), Some("a.txt"));
    assert_eq!(step(&walk, "c.txt", true), None);
    assert_eq!(step(&walk, "gone.txt", true), None);
}

#[test]
fn the_marked_files_only_and_none_for_a_single_file() {
    let marked = Walk::of(ListerFiles::Marked, vec!["a.txt".into(), "c.txt".into()]);
    assert_eq!(step(&marked, "a.txt", true).as_deref(), Some("c.txt"));
    assert_eq!(step(&marked, "c.txt", false).as_deref(), Some("a.txt"));
    // From a file that is not marked the next marked one is the step.
    assert_eq!(step(&marked, "b.txt", true).as_deref(), Some("c.txt"));
    let single = Walk::of(ListerFiles::Single, vec!["a.txt".into(), "c.txt".into()]);
    assert_eq!(step(&single, "a.txt", true), None);
    assert_eq!(step(&single, "c.txt", false), None);
}
