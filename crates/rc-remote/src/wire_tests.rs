use std::path::{Path, PathBuf};

use super::{Refusals, absolute, child, home};

const REFUSALS: Refusals = Refusals {
    relative: "relative",
    prefix: "prefix",
    parent: "parent",
};

fn wire(path: &Path) -> rc_vfs::Result<String> {
    absolute(path, &REFUSALS, |_| Ok(()))
}

#[test]
fn a_backend_path_becomes_an_absolute_wire_path() {
    assert_eq!(wire(Path::new("/")).expect("root"), "/");
    assert_eq!(wire(Path::new("/a/./b/")).expect("dot"), "/a/b");
    let joined = Path::new("/x").join("y");
    assert_eq!(wire(&joined).expect("joined"), "/x/y");
}

#[test]
fn the_protocol_names_each_refusal() {
    let reason = |text: &str| match wire(Path::new(text)) {
        Err(rc_vfs::VfsError::InvalidPath { reason, .. }) => reason,
        other => panic!("{text:?}: {other:?}"),
    };
    assert_eq!(reason("relative/x"), "relative");
    assert_eq!(reason("/a/../b"), "parent");
    let refused = absolute(Path::new("/a/bad"), &REFUSALS, |name| {
        if name == "bad" {
            Err(rc_vfs::VfsError::InvalidPath {
                path: PathBuf::from(name),
                reason: "bad",
            })
        } else {
            Ok(())
        }
    });
    assert!(refused.is_err(), "the name check runs on every name");
}

#[test]
fn a_child_is_joined_with_one_slash() {
    assert_eq!(child("/", "a"), "/a");
    assert_eq!(child("/srv/www", "a b"), "/srv/www/a b");
    assert_eq!(child("/srv/www/", "x"), "/srv/www/x");
}

#[test]
fn the_home_is_a_plain_absolute_path_or_the_root() {
    assert_eq!(home(Some("/home/web")), PathBuf::from("/home/web"));
    assert_eq!(home(Some("//srv//www/")), PathBuf::from("/srv/www"));
    for hostile in [
        "home/web",
        "C:/inetpub",
        "/a\r\nDELE b",
        "/a\u{1b}[2J",
        "/home/../etc",
        "/a\\b",
        "",
    ] {
        assert_eq!(home(Some(hostile)), PathBuf::from("/"), "{hostile:?}");
    }
    assert_eq!(home(None), PathBuf::from("/"));
}
