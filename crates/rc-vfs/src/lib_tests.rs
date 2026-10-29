use std::{path::PathBuf, sync::Arc};

use proptest::prelude::*;

use super::{BackendId, LocalFs, Vfs, VfsPath};

#[test]
fn a_local_path_joins_and_climbs_on_its_backend() {
    let dir = VfsPath::local("/data/music");
    let file = dir.join("track.flac");
    assert_eq!(file.backend, BackendId::Local);
    assert_eq!(file.path, PathBuf::from("/data/music/track.flac"));
    assert_eq!(file.parent(), Some(dir));
}

#[test]
fn the_root_has_no_parent() {
    assert_eq!(VfsPath::local("/").parent(), None);
}

#[test]
fn the_root_of_an_archive_climbs_to_the_folder_of_the_archive_file() {
    let outer = VfsPath::local("/data/backup.zip");
    let root = VfsPath::archive(outer, super::ArchiveFormat::Zip, "/");
    assert_eq!(root.parent(), Some(VfsPath::local("/data")));
    let docs = root.join("docs");
    assert_eq!(docs.parent(), Some(root));
}

#[test]
fn a_vfs_path_round_trips_through_json() {
    let path = VfsPath::local("/data");
    let json = serde_json::to_string(&path).expect("serialize");
    assert_eq!(json, r#"{"backend":"local","path":"/data"}"#);
    let back: VfsPath = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(back, path);
}

proptest! {
    /// The JSON form is the stable text form (history, hotlist, MCP): any path, whatever its
    /// characters, comes back from it unchanged and writes the same text again.
    #[test]
    fn the_json_form_round_trips_any_path(path in any::<String>()) {
        let text = format!(
            r#"{{"backend":"local","path":{}}}"#,
            serde_json::to_string(&path).expect("a string serializes")
        );
        let parsed: VfsPath = serde_json::from_str(&text).expect("parse");
        prop_assert_eq!(&parsed, &VfsPath::local(&path));
        prop_assert_eq!(serde_json::to_string(&parsed).expect("serialize"), text);
    }
}

#[test]
fn a_backend_is_usable_as_a_trait_object() {
    let backend: Arc<dyn Vfs> = Arc::new(LocalFs::new());
    assert!(backend.capabilities().rename_across_dirs);
}
