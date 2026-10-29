use std::fs;

use super::{generated, prune_backups};

/// The owner's decision of 2026-10-28: the oldest automatic backups beyond `keep` go, oldest
/// first; a name a person gave, a safety backup and a folder stay.
#[test]
fn the_oldest_automatic_backups_beyond_keep_are_removed() {
    let dir = tempfile::tempdir().expect("temp dir");
    let names = [
        "rcommander-20261001-090000.db",
        "rcommander-20261002-090000.db",
        "rcommander-20261002-090000-2.db",
        "rcommander-20261003-120000.db",
        // A person's name, a restore's safety backup, a folder: never removed.
        "before-the-upgrade.db",
        "rcommander-20260901-080000-before-restore.db",
    ];
    for name in names {
        fs::write(dir.path().join(name), b"x").expect("write");
    }
    fs::create_dir(dir.path().join("rcommander-20250101-000000.db")).expect("mkdir");
    let removed = prune_backups(dir.path(), 2).expect("pruned");
    assert_eq!(
        removed,
        [
            dir.path().join("rcommander-20261001-090000.db"),
            dir.path().join("rcommander-20261002-090000.db"),
        ]
    );
    for kept in &names[2..] {
        assert!(dir.path().join(kept).exists(), "{kept}");
    }
    assert!(dir.path().join("rcommander-20250101-000000.db").is_dir());
    // Fewer than `keep`: nothing goes; `keep` 0 is held to 1.
    assert!(prune_backups(dir.path(), 10).expect("pruned").is_empty());
    let last = prune_backups(dir.path(), 0).expect("pruned");
    assert_eq!(last, [dir.path().join("rcommander-20261002-090000-2.db")]);
    assert!(dir.path().join("rcommander-20261003-120000.db").exists());
}

#[test]
fn only_generated_names_have_an_order() {
    assert_eq!(
        generated("rcommander-20261002-090000.db"),
        Some(("20261002-090000".to_owned(), 1))
    );
    assert_eq!(
        generated("rcommander-20261002-090000-12.db"),
        Some(("20261002-090000".to_owned(), 12))
    );
    for name in [
        "rcommander.db",
        "rcommander-2026100-090000.db",
        "rcommander-20261002-090000-before-restore.db",
        "rcommander-20261002-0900ab.db",
        "rcommander-20261002-090000.db.partial",
        "my-rcommander-20261002-090000.db",
    ] {
        assert_eq!(generated(name), None, "{name}");
    }
}
