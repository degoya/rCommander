use std::path::{Path, PathBuf};

use super::{Kind, Start, start};

/// An absolute folder on every system, and a file system holding only `root` and `root/keys`.
fn root() -> PathBuf {
    std::env::temp_dir().join("rc-browse")
}

fn is_dir(path: &Path) -> bool {
    path == root() || path == root().join("keys")
}

fn text(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

#[test]
fn a_folder_the_field_names_is_where_the_dialog_opens() {
    let keys = root().join("keys");
    for kind in [Kind::Open, Kind::Save, Kind::Folder] {
        assert_eq!(
            start(kind, &text(&keys), None, is_dir),
            Start {
                folder: Some(keys.clone()),
                name: None,
            }
        );
    }
}

#[test]
fn a_file_opens_its_folder_and_is_suggested_by_name() {
    let file = root().join("keys").join("id_ed25519");
    for kind in [Kind::Open, Kind::Save] {
        assert_eq!(
            start(kind, &text(&file), None, is_dir),
            Start {
                folder: Some(root().join("keys")),
                name: Some("id_ed25519".to_owned()),
            }
        );
    }
    assert_eq!(
        start(Kind::Folder, &text(&file), None, is_dir).name,
        None,
        "a folder dialog suggests no file name"
    );
}

#[test]
fn a_missing_folder_falls_back_to_the_nearest_one_that_exists() {
    let backup = root().join("rCommander Backups").join("rcommander-1.db");
    assert_eq!(
        start(Kind::Save, &text(&backup), None, is_dir),
        Start {
            folder: Some(root()),
            name: Some("rcommander-1.db".to_owned()),
        }
    );
}

#[test]
fn an_empty_or_relative_field_opens_the_fallback_if_it_exists() {
    let fallback = Some(root().join("keys"));
    for current in ["", "   ", "id_rsa", "keys/id_rsa"] {
        assert_eq!(
            start(Kind::Open, current, fallback.clone(), is_dir),
            Start {
                folder: fallback.clone(),
                name: None,
            },
            "{current:?}"
        );
    }
    let missing = Some(root().join("nowhere"));
    assert_eq!(start(Kind::Open, "", missing, is_dir).folder, None);
}
