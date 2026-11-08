use std::path::{Path, PathBuf};

use rc_vfs::{Attributes, Details, Entry, EntryKind, Owner};

use super::{link_destination, local_stamp, needs_in_place, runs_as_program};

const PATHEXT: &str = ".COM;.EXE;.BAT;.CMD";

fn entry(owner: Option<&str>) -> Entry {
    Entry {
        name: "app.conf".into(),
        kind: EntryKind::File,
        size: 3,
        modified: None,
        created: None,
        accessed: None,
        attributes: Attributes::from_mode(0o600),
        hidden: false,
        owner: owner.map(|user| Owner {
            user: user.into(),
            group: user.into(),
        }),
    }
}

#[test]
fn on_windows_an_extension_of_pathext_runs() {
    for name in ["tool.exe", "TOOL.EXE", "setup.Cmd", "x.bat"] {
        assert!(runs_as_program(name, None, true, PATHEXT), "{name}");
    }
    for name in ["notes.txt", "exe", "tool.exe.txt", "noext"] {
        assert!(!runs_as_program(name, Some(0o755), true, PATHEXT), "{name}");
    }
    assert!(
        !runs_as_program("tool.exe", None, true, ""),
        "nothing listed"
    );
}

#[test]
fn elsewhere_an_execute_bit_runs() {
    assert!(runs_as_program("deploy.sh", Some(0o755), false, PATHEXT));
    assert!(runs_as_program("deploy", Some(0o701), false, PATHEXT));
    assert!(!runs_as_program("tool.exe", Some(0o644), false, PATHEXT));
    assert!(
        !runs_as_program("tool", None, false, PATHEXT),
        "no mode known"
    );
}

#[test]
fn several_links_or_another_owner_write_in_place() {
    let two = Details {
        links: Some(2),
        ..Details::default()
    };
    assert!(needs_in_place(&entry(None), &two, Some("alice")));
    let one = Details {
        links: Some(1),
        ..Details::default()
    };
    assert!(!needs_in_place(&entry(Some("alice")), &one, Some("alice")));
    assert!(needs_in_place(&entry(Some("root")), &one, Some("alice")));
    // A number is not compared with a name; without a user nothing is.
    assert!(!needs_in_place(&entry(Some("0")), &one, Some("alice")));
    assert!(!needs_in_place(&entry(Some("root")), &one, None));
}

#[test]
fn a_link_leads_from_its_own_folder() {
    let link = Path::new("/etc/nginx/sites-enabled/site");
    assert_eq!(
        link_destination(link, Path::new("../sites-available/site")),
        PathBuf::from("/etc/nginx/sites-available/site")
    );
    assert_eq!(
        link_destination(link, Path::new("/srv/site.conf")),
        PathBuf::from("/srv/site.conf")
    );
    assert_eq!(
        link_destination(Path::new("/a"), Path::new("../../b")),
        PathBuf::from("/b"),
        "never above the root"
    );
}

#[test]
fn a_copy_has_a_stamp_while_it_is_a_file() {
    let dir = tempfile::tempdir().expect("temp dir");
    let file = dir.path().join("a.txt");
    assert_eq!(local_stamp(&file), None);
    std::fs::write(&file, b"abc").expect("write");
    assert_eq!(local_stamp(&file).map(|stamp| stamp.size), Some(3));
    assert_eq!(local_stamp(dir.path()), None, "a folder is no copy");
}
