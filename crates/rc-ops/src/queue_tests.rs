use std::path::Path;

use rc_vfs::VfsPath;

use super::{check_placement, check_placement_as, one_backend};

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

/// A WSL distribution behind Windows' local backend, which says of itself that case does not
/// count (RC-110-26): there `Makefile` and `makefile` are two files.
#[test]
fn case_counts_inside_a_wsl_distribution() {
    let wsl = |rest: &str| format!("//wsl.localhost/Ubuntu/home/me/{rest}");
    let (source, lower, inside) = (wsl("Src/x"), wsl("src"), wsl("src/X/deeper"));
    assert!(check_placement(Path::new(&source), Path::new(&lower), false).is_ok());
    assert!(check_placement(Path::new(&wsl("src/x")), Path::new(&inside), false).is_ok());
    let same = check_placement(Path::new(&source), Path::new(&wsl("Src")), false);
    assert_eq!(same.map_err(|e| e.code()), Err("same_location"));
    let (from, to) = (wsl("Makefile"), wsl("makefile"));
    for moving in [false, true] {
        assert!(check_placement_as(Path::new(&from), Path::new(&to), false, moving).is_ok());
    }
    let onto = check_placement_as(Path::new(&from), Path::new(&from), false, false);
    assert_eq!(onto.map_err(|e| e.code()), Err("same_location"));
    // Elsewhere on that backend the copy onto another case of itself stays refused.
    let folded = check_placement_as(
        Path::new("/a/Makefile"),
        Path::new("/a/makefile"),
        false,
        false,
    );
    assert_eq!(folded.map_err(|e| e.code()), Err("same_location"));
}

#[test]
fn sources_need_one_backend() {
    assert_eq!(one_backend(&[]).map_err(|e| e.code()), Err("no_sources"));
    let sources = [VfsPath::local("/a"), VfsPath::local("/b")];
    assert!(one_backend(&sources).is_ok());
}
