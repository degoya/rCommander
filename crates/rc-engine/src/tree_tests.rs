use std::sync::Arc;

use rc_vfs::VfsPath;

use super::{SeparateTree, TreeChild, TreeLevel, tree_chain, tree_root};

#[test]
fn the_root_is_the_location_s_top_and_the_chain_leads_down_to_the_folder() {
    let path = VfsPath::local("/home/tester/docs");
    assert_eq!(tree_root(&path), VfsPath::local("/"));
    assert_eq!(
        tree_chain(&path),
        [
            VfsPath::local("/"),
            VfsPath::local("/home"),
            VfsPath::local("/home/tester"),
            VfsPath::local("/home/tester/docs"),
        ]
    );
    assert_eq!(tree_chain(&VfsPath::local("/")), [VfsPath::local("/")]);
}

#[cfg(windows)]
#[test]
fn on_windows_the_root_is_the_drive_or_the_share() {
    assert_eq!(
        tree_root(&VfsPath::local(r"C:\Windows\WinSxS")),
        VfsPath::local(r"C:\")
    );
    assert_eq!(
        tree_root(&VfsPath::local(r"\\wsl.localhost\Ubuntu\home")),
        VfsPath::local(r"\\wsl.localhost\Ubuntu\")
    );
}

#[test]
fn ctrl_shift_f8_turns_off_one_two_and_off_again() {
    let mut turned = vec![SeparateTree::Off];
    for _ in 0..3 {
        let last = *turned.last().expect("one");
        turned.push(last.next());
    }
    assert_eq!(
        turned,
        [
            SeparateTree::Off,
            SeparateTree::One,
            SeparateTree::Two,
            SeparateTree::Off
        ]
    );
    for tree in [SeparateTree::Off, SeparateTree::One, SeparateTree::Two] {
        assert_eq!(SeparateTree::from_setting(tree.as_str()), tree);
    }
    assert_eq!(SeparateTree::from_setting("three"), SeparateTree::Off);
}

#[test]
fn hidden_folders_show_only_with_hidden_entries() {
    let child = |name: &str, hidden| TreeChild {
        name: name.to_owned(),
        hidden,
        link: false,
    };
    let level = TreeLevel {
        path: VfsPath::local("/"),
        children: Arc::from([child(".git", true), child("src", false)]),
        truncated: false,
    };
    let names = |level: TreeLevel| -> Vec<String> {
        level.children.iter().map(|c| c.name.clone()).collect()
    };
    assert_eq!(names(level.shown(false)), ["src"]);
    assert_eq!(names(level.shown(true)), [".git", "src"]);
}
