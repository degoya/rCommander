use std::path::PathBuf;

use rc_vfs::{Attributes, BackendId, Entry, EntryKind, VfsPath};

use super::*;

fn entry(name: &str, dir: bool) -> Entry {
    Entry {
        name: name.to_owned(),
        kind: if dir { EntryKind::Dir } else { EntryKind::File },
        size: 0,
        modified: None,
        created: None,
        accessed: None,
        attributes: Attributes::default(),
        hidden: false,
        owner: None,
    }
}

fn local(path: &str) -> VfsPath {
    VfsPath {
        backend: BackendId::Local,
        path: PathBuf::from(path),
    }
}

fn no_variables(_: &str) -> Option<String> {
    None
}

fn list(text: &str) -> IgnoreList {
    IgnoreList::parse_with(text, &no_variables)
}

#[test]
fn a_name_entry_hides_everywhere() {
    let list = list("*.bak\nThumbs.db");
    for dir in [local("/data"), local("/data/deep/er")] {
        assert!(list.hides(&dir, &entry("old.BAK", false)));
        assert!(list.hides(&dir, &entry("thumbs.db", false)));
        assert!(!list.hides(&dir, &entry("old.bak.txt", false)));
    }
    let server = VfsPath {
        backend: BackendId::Sftp { connection_id: 1 },
        path: PathBuf::from("/srv"),
    };
    assert!(
        list.hides(&server, &entry("x.bak", false)),
        "names count on servers too"
    );
    assert!(
        list.hides(&local("/data"), &entry("sub\\old.bak", false)),
        "a flat view's name is judged by its own part"
    );
}

#[test]
fn a_path_entry_hides_only_that_folder() {
    let (text, inside, beside) = if cfg!(windows) {
        ("C:\\Users\\me\\AppData", "C:\\Users\\me", "C:\\Users\\you")
    } else {
        ("/home/me/AppData", "/home/me", "/home/you")
    };
    let list = list(text);
    assert!(list.hides(&local(inside), &entry("appdata", true)));
    assert!(!list.hides(&local(beside), &entry("AppData", true)));
    let server = VfsPath {
        backend: BackendId::Sftp { connection_id: 1 },
        path: PathBuf::from(inside),
    };
    assert!(
        !list.hides(&server, &entry("AppData", true)),
        "a path counts on this machine only"
    );
}

/// An entry with a separator but neither a root nor a drive ends a path.
fn list_text_tail() -> IgnoreList {
    list("node_modules\\.cache")
}

#[test]
fn a_relative_path_entry_ends_a_path() {
    let list = list_text_tail();
    let dir = if cfg!(windows) {
        "C:\\src\\app\\node_modules"
    } else {
        "/src/app/node_modules"
    };
    assert!(list.hides(&local(dir), &entry(".cache", true)));
    assert!(!list.hides(&local(dir), &entry(".config", true)));
}

#[test]
fn a_rooted_entry_hides_on_every_drive() {
    let list = list("\\$RECYCLE.BIN");
    if cfg!(windows) {
        assert!(list.hides(&local("C:\\"), &entry("$Recycle.Bin", true)));
        assert!(list.hides(&local("D:\\"), &entry("$RECYCLE.BIN", true)));
        assert!(list.hides(&local("\\\\nas\\share"), &entry("$RECYCLE.BIN", true)));
        assert!(!list.hides(&local("D:\\sub"), &entry("$RECYCLE.BIN", true)));
    } else {
        assert!(list.hides(&local("/"), &entry("$RECYCLE.BIN", true)));
        assert!(!list.hides(&local("/sub"), &entry("$RECYCLE.BIN", true)));
    }
    assert_eq!(
        without_root(&"c:/x/y".chars().collect::<Vec<_>>()),
        &['/', 'x', '/', 'y']
    );
    assert_eq!(
        without_root(&"//nas/share/x".chars().collect::<Vec<_>>()),
        &['/', 'x']
    );
}

#[test]
fn a_trailing_backslash_matches_only_folders() {
    let list = list("build\\");
    assert!(list.hides(&local("/p"), &entry("build", true)));
    assert!(!list.hides(&local("/p"), &entry("build", false)));
}

#[test]
fn environment_variables_expand() {
    let lookup = |name: &str| (name == "USERPROFILE").then(|| "C:\\Users\\me".to_owned());
    let list = IgnoreList::parse_with("%USERPROFILE%\\AppData\n%UNKNOWN%\\x", &lookup);
    assert_eq!(list.rules.len(), 2);
    assert_eq!(list.rules[0].scope, Scope::Path);
    let pattern: String = list.rules[0].pattern.iter().collect();
    assert_eq!(pattern, "c:/users/me/appdata");
    // Not set: stays as written, as Total Commander leaves it.
    let pattern: String = list.rules[1].pattern.iter().collect();
    assert_eq!(pattern, "*/%unknown%/x");
}

#[test]
fn an_overlong_list_is_refused() {
    let lines = vec!["*.bak"; MAX_IGNORE_LINES + 1].join("\n");
    assert!(checked_lines(&lines).is_err());
    assert!(checked_lines(&"x".repeat(MAX_IGNORE_BYTES + 1)).is_err());
    assert!(checked_lines("a\u{7}b").is_err());
    assert_eq!(
        checked_lines("  *.bak \r\n\r\nThumbs.db\n").as_deref(),
        Ok("*.bak\nThumbs.db")
    );
    let fits = vec!["*.bak"; MAX_IGNORE_LINES].join("\n");
    assert!(checked_lines(&fits).is_ok());
}

#[test]
fn the_list_in_force_is_the_process_one() {
    set_active(Some(Arc::new(list("*.bak"))));
    assert!(ignored(&local("/x"), &entry("a.bak", false)));
    let mut entries = vec![entry("a.bak", false), entry("a.txt", false)];
    retain_shown(&local("/x"), &mut entries);
    assert_eq!(entries.len(), 1);
    set_active(Some(Arc::new(list(""))));
    assert!(active().is_none(), "an empty list is none");
    set_active(None);
    assert!(!ignored(&local("/x"), &entry("a.bak", false)));
}
