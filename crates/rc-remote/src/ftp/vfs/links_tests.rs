use std::path::PathBuf;

use rc_vfs::{Attributes, Entry, EntryKind, TargetKind};

use super::{Links, unresolved_link};

fn link(name: &str) -> Entry {
    Entry {
        name: name.to_owned(),
        kind: EntryKind::Symlink {
            target: PathBuf::from("/elsewhere"),
            target_kind: None,
        },
        size: 10,
        modified: None,
        created: None,
        attributes: Attributes::default(),
        hidden: false,
        owner: None,
    }
}

/// A link a listing left open is noted; once a look-up learned it, the next listing shows it.
#[test]
fn a_learned_link_shows_in_the_next_listing() {
    let links = Links::default();
    let mut entries = vec![link("docs"), link("dangling")];
    links.fill("/srv", &mut entries);
    assert!(entries.iter().all(unresolved_link));
    assert!(links.listed("/srv/docs"));
    assert!(!links.listed("/srv/other"));
    links.learn("/srv/docs", TargetKind::Dir);
    let mut again = vec![link("docs"), link("dangling")];
    links.fill("/srv", &mut again);
    assert!(again[0].is_dir_like());
    assert!(unresolved_link(&again[1]));
    // In another folder the same name is another link.
    let mut elsewhere = vec![link("docs")];
    links.fill("/", &mut elsewhere);
    assert!(unresolved_link(&elsewhere[0]));
}
