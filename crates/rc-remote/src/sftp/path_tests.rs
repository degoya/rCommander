use std::path::{Path, PathBuf};

use super::*;

#[test]
fn a_backend_path_is_sent_with_forward_slashes() {
    assert_eq!(wire_path(Path::new("/")).expect("root"), "/");
    assert_eq!(
        wire_path(&Path::new("/home").join("tester").join("a b.txt")).expect("path"),
        "/home/tester/a b.txt"
    );
    assert_eq!(wire_path(Path::new("/srv/./www")).expect("dot"), "/srv/www");
}

#[test]
fn relative_parent_and_nul_paths_are_refused_before_sending() {
    for path in ["relative/x", "/srv/../etc", "/srv/a\0b"] {
        let code = wire_path(Path::new(path)).expect_err(path).code();
        assert_eq!(code, "invalid_path", "{path:?}");
    }
}

#[test]
fn the_home_the_server_names_must_be_a_plain_absolute_path() {
    assert_eq!(
        from_wire("/home/tester"),
        Some(PathBuf::from("/home/tester"))
    );
    assert_eq!(from_wire("/"), Some(PathBuf::from("/")));
    assert_eq!(from_wire("//srv//x/"), Some(PathBuf::from("/srv/x")));
    for hostile in ["home", "/home/../etc", "/a\0b", "/a\\b", ""] {
        assert_eq!(from_wire(hostile), None, "{hostile:?}");
    }
}
