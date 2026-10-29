use super::*;

#[test]
fn write_atomically_replaces_and_leaves_no_temporary() {
    let directory = tempfile::tempdir().expect("tempdir");
    let path = directory.path().join("journal.json");
    write_atomically(&path, b"{\"a\":1}").expect("first");
    write_atomically(&path, b"{\"a\":2}").expect("second");
    assert_eq!(fs::read(&path).expect("read"), b"{\"a\":2}");
    assert!(!exists(&directory.path().join("journal.json.tmp")));
}

#[test]
fn remove_any_takes_files_trees_and_nothing() {
    let directory = tempfile::tempdir().expect("tempdir");
    let tree = directory.path().join("tree");
    fs::create_dir_all(tree.join("inner")).expect("tree");
    fs::write(tree.join("inner/file"), b"x").expect("file");
    let file = directory.path().join("file");
    fs::write(&file, b"x").expect("file");
    remove_any(&tree).expect("tree");
    remove_any(&file).expect("file");
    remove_any(&directory.path().join("absent")).expect("absent");
    assert!(!exists(&tree) && !exists(&file));
}

#[test]
fn a_rename_without_a_wait_answers_at_once() {
    let directory = tempfile::tempdir().expect("tempdir");
    let missing = directory.path().join("missing");
    assert!(rename_within(&missing, &directory.path().join("b"), Duration::ZERO).is_err());
}
