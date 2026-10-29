use std::{
    cmp::Ordering,
    time::{Duration, SystemTime},
};

use rc_vfs::{Attributes, Entry, EntryKind};

use super::*;

fn entry(name: &str, dir: bool, size: u64, age: u64) -> Entry {
    Entry {
        name: name.to_owned(),
        kind: if dir { EntryKind::Dir } else { EntryKind::File },
        size,
        modified: Some(SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000 - age)),
        created: None,
        attributes: Attributes::default(),
        hidden: false,
        owner: None,
    }
}

/// `src/` and `docs/` (directories), then four files: name, size, age in seconds.
fn listing() -> Vec<Entry> {
    vec![
        entry("file10.txt", false, 300, 30),
        entry("src", true, 0, 5),
        entry("File2.md", false, 100, 10),
        entry("b.rs", false, 200, 40),
        entry("docs", true, 0, 50),
        entry("a", false, 400, 20),
    ]
}

fn sorted(order: SortOrder) -> Vec<String> {
    let mut entries = listing();
    sort_entries(&mut entries, order, Names::WINDOWS);
    entries.into_iter().map(|entry| entry.name).collect()
}

fn order(key: SortKey, descending: bool) -> SortOrder {
    SortOrder { key, descending }
}

#[test]
fn names_compare_naturally_and_without_case() {
    assert_eq!(natural_cmp("file2", "file10"), Ordering::Less);
    assert_eq!(natural_cmp("File2", "file10"), Ordering::Less);
    assert_eq!(natural_cmp("a", "B"), Ordering::Less);
    assert_eq!(natural_cmp("x9y", "x09y"), Ordering::Equal);
    assert_eq!(natural_cmp("abc", "ab"), Ordering::Greater);
    assert_eq!(natural_cmp("", ""), Ordering::Equal);
    assert_eq!(natural_cmp("img99", "img100a"), Ordering::Less);
    assert_eq!(
        natural_cmp("v12345678901234567890", "v9"),
        Ordering::Greater,
        "digit runs longer than any integer"
    );
}

/// Total Commander shows Windows' order: `_` and other symbols before digits, digits before
/// letters (owner's Windows test 2026-10-18).
#[test]
fn symbols_come_before_digits_and_digits_before_letters() {
    let mut names = [
        "zeta",
        "2024",
        "_drafts",
        "$RECYCLE.BIN",
        "[old]",
        "10",
        "Alpha",
        "_a",
    ];
    names.sort_by(|a, b| natural_cmp(a, b));
    assert_eq!(
        names,
        [
            "$RECYCLE.BIN",
            "[old]",
            "_a",
            "_drafts",
            "10",
            "2024",
            "Alpha",
            "zeta"
        ]
    );
    assert_eq!(natural_cmp("_x", "1x"), Ordering::Less);
    assert_eq!(natural_cmp("9", "a"), Ordering::Less);
    assert_eq!(natural_cmp("a_b", "a1b"), Ordering::Less);
}

/// The owner's drive in Total Commander (2026-10-19): the hyphens of `--- incoming ---` are
/// passed over, so it sorts by its leading blank, before every other symbol; names otherwise
/// equal are told apart by their hyphens.
#[test]
fn hyphens_and_apostrophes_count_only_between_otherwise_equal_names() {
    let mut names = [
        "test",
        "System Volume Information",
        "rdownloader-windows-x86_64",
        "rdownloader-capture-windows-x86_64",
        "mp3",
        "_fix",
        "_AI_Tracks",
        "_2stick",
        "$RECYCLE.BIN",
        "--- incoming ---",
    ];
    names.sort_by(|a, b| natural_cmp(a, b));
    assert_eq!(
        names,
        [
            "--- incoming ---",
            "$RECYCLE.BIN",
            "_2stick",
            "_AI_Tracks",
            "_fix",
            "mp3",
            "rdownloader-capture-windows-x86_64",
            "rdownloader-windows-x86_64",
            "System Volume Information",
            "test",
        ]
    );
    assert_eq!(natural_cmp("don't", "dont"), Ordering::Greater);
    assert_eq!(natural_cmp("re-do", "redo"), Ordering::Greater);
    assert_ne!(natural_cmp("re-do", "redo"), Ordering::Equal);
}

#[test]
fn every_key_sorts_both_ways_with_directories_on_top() {
    assert_eq!(
        sorted(order(SortKey::Name, false)),
        ["docs", "src", "a", "b.rs", "File2.md", "file10.txt"]
    );
    assert_eq!(
        sorted(order(SortKey::Name, true)),
        ["src", "docs", "file10.txt", "File2.md", "b.rs", "a"]
    );
    assert_eq!(
        sorted(order(SortKey::Ext, false)),
        ["docs", "src", "a", "File2.md", "b.rs", "file10.txt"]
    );
    assert_eq!(
        sorted(order(SortKey::Ext, true)),
        ["docs", "src", "file10.txt", "b.rs", "File2.md", "a"]
    );
    assert_eq!(
        sorted(order(SortKey::Size, false)),
        ["docs", "src", "File2.md", "b.rs", "file10.txt", "a"]
    );
    assert_eq!(
        sorted(order(SortKey::Size, true)),
        ["docs", "src", "a", "file10.txt", "b.rs", "File2.md"]
    );
    // Newest first is descending: src is younger than docs.
    assert_eq!(
        sorted(order(SortKey::Date, true)),
        ["src", "docs", "File2.md", "a", "file10.txt", "b.rs"]
    );
    assert_eq!(
        sorted(order(SortKey::Date, false)),
        ["docs", "src", "b.rs", "file10.txt", "a", "File2.md"]
    );
}

#[test]
fn choosing_a_key_again_turns_the_direction_around() {
    let by_name = SortOrder::default();
    assert_eq!(by_name, order(SortKey::Name, false));
    assert_eq!(by_name.choose(SortKey::Name), order(SortKey::Name, true));
    assert_eq!(by_name.choose(SortKey::Size), order(SortKey::Size, true));
    assert_eq!(by_name.choose(SortKey::Date), order(SortKey::Date, true));
    assert_eq!(
        by_name.choose(SortKey::Date).choose(SortKey::Date),
        order(SortKey::Date, false)
    );
    assert_eq!(by_name.choose(SortKey::Ext), order(SortKey::Ext, false));
}

#[test]
fn the_extension_is_a_files_last_dot_but_not_a_leading_one() {
    assert_eq!(extension(&entry("a.tar.gz", false, 0, 0)), "gz");
    assert_eq!(extension(&entry(".bashrc", false, 0, 0)), "");
    assert_eq!(extension(&entry("noext", false, 0, 0)), "");
    assert_eq!(extension(&entry("dir.d", true, 0, 0)), "");
    // A flat view names a file with its folders: only the last part has the extension.
    let nested = format!("sub.d{}file", std::path::MAIN_SEPARATOR);
    assert_eq!(extension(&entry(&nested, false, 0, 0)), "");
    let hidden = format!("sub{}.bashrc", std::path::MAIN_SEPARATOR);
    assert_eq!(extension(&entry(&hidden, false, 0, 0)), "");
    let both = format!("a.b{}c.txt", std::path::MAIN_SEPARATOR);
    assert_eq!(extension(&entry(&both, false, 0, 0)), "txt");
}

/// `sort.names` linux: the byte order of `ls` in the C locale - capitals before small letters,
/// symbols by their code, hyphens as any other character; digits by value only with
/// `sort.numbers_by_value` (owner, 2026-10-19).
#[test]
fn linux_order_is_byte_order_with_numbers_by_value_on_request() {
    let mut names = ["b", "_x", "B", "--- in", "a10", "a2", "$R"];
    let linux = Names {
        linux: true,
        numbers: true,
    };
    names.sort_by(|a, b| compare_names(a, b, linux));
    assert_eq!(names, ["$R", "--- in", "B", "_x", "a2", "a10", "b"]);
    let strict = Names {
        linux: true,
        numbers: false,
    };
    names.sort_by(|a, b| compare_names(a, b, strict));
    assert_eq!(names, ["$R", "--- in", "B", "_x", "a10", "a2", "b"]);
    let windows_strict = Names {
        linux: false,
        numbers: false,
    };
    assert_eq!(compare_names("a10", "a2", windows_strict), Ordering::Less);
    assert_eq!(
        compare_names("a10", "a2", Names::WINDOWS),
        Ordering::Greater
    );
}

/// `by_target`: Linux' order on a server, inside WSL and in an archive lying there; Windows' on
/// this Windows machine's drives (Linux' when it is none).
#[test]
fn by_target_takes_the_order_of_the_system_the_panel_shows() {
    set_names(NameOrder::ByTarget, true);
    let server = VfsPath {
        backend: BackendId::Sftp { connection_id: 1 },
        path: "/home".into(),
    };
    assert!(names_for(&server).linux);
    let wsl = VfsPath::local(r"\\wsl.localhost\Debian\home");
    let local = VfsPath::local(r"C:\Users");
    if cfg!(windows) {
        assert!(names_for(&wsl).linux);
        assert!(!names_for(&local).linux);
    } else {
        assert!(names_for(&local).linux);
    }
    set_names(NameOrder::Windows, false);
    assert_eq!(
        names_for(&server),
        Names {
            linux: false,
            numbers: false
        }
    );
    set_names(NameOrder::Windows, true);
}
