use super::*;

fn titles_of(paths: &[&str]) -> Vec<String> {
    let paths: Vec<VfsPath> = paths.iter().map(|path| VfsPath::local(*path)).collect();
    let refs: Vec<&VfsPath> = paths.iter().collect();
    titles(&refs)
}

#[test]
fn a_title_is_the_folders_last_part() {
    assert_eq!(titles_of(&["/home/me/src", "/tmp"]), ["src", "tmp"]);
}

#[test]
fn a_root_shows_itself() {
    assert_eq!(titles_of(&["/"]), ["/"]);
}

#[test]
fn equal_names_get_their_parent() {
    assert_eq!(
        titles_of(&["/work/rc-db/src", "/work/rc-vfs/src", "/work/docs"]),
        ["src (rc-db)", "src (rc-vfs)", "docs"]
    );
}

#[test]
fn what_is_still_equal_is_numbered() {
    assert_eq!(
        titles_of(&["/a/src", "/a/src", "/b/a/src"]),
        ["src (a)", "src (a) (2)", "src (a) (3)"]
    );
}
