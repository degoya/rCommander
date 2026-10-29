use std::path::Path;

use rc_vfs::VfsPath;

use super::{check_placement, one_backend};

#[test]
fn a_copy_onto_itself_or_into_itself_is_refused() {
    let into_parent = check_placement(Path::new("/a/x"), Path::new("/a"), true);
    assert_eq!(into_parent.map_err(|e| e.code()), Err("same_location"));
    let into_itself = check_placement(Path::new("/a/x"), Path::new("/a/x/deeper"), true);
    assert_eq!(
        into_itself.map_err(|e| e.code()),
        Err("target_inside_source")
    );
    assert!(check_placement(Path::new("/a/x"), Path::new("/a/xy"), true).is_ok());
    assert!(check_placement(Path::new("/a/x"), Path::new("/b"), true).is_ok());
}

#[test]
fn case_counts_only_where_the_backend_says_so() {
    assert!(check_placement(Path::new("/A/x"), Path::new("/a"), true).is_ok());
    let folded = check_placement(Path::new("/A/x"), Path::new("/a"), false);
    assert_eq!(folded.map_err(|e| e.code()), Err("same_location"));
}

#[test]
fn sources_need_one_backend() {
    assert_eq!(one_backend(&[]).map_err(|e| e.code()), Err("no_sources"));
    let sources = [VfsPath::local("/a"), VfsPath::local("/b")];
    assert!(one_backend(&sources).is_ok());
}
