use std::path::PathBuf;

use iced::widget::image::Handle;
use rc_engine::{Entry, EntryKind, VfsPath};
use rc_vfs::Attributes;

use super::{IconMode, Icons, key::IconKey, loader::Loaded};

fn entry(name: &str, kind: EntryKind) -> Entry {
    Entry {
        name: name.to_owned(),
        kind,
        size: 0,
        modified: None,
        created: None,
        attributes: Attributes::default(),
        hidden: false,
        owner: None,
    }
}

fn dir() -> VfsPath {
    VfsPath::local("/data/work")
}

/// Icons with native ones in reach, as on Windows.
fn native() -> Icons {
    Icons {
        unavailable: false,
        ..Icons::default()
    }
}

fn pixel() -> Handle {
    Handle::from_rgba(1, 1, vec![0u8; 4])
}

#[test]
fn the_setting_names_the_mode_and_native_is_the_default() {
    assert_eq!(IconMode::from_setting("off"), IconMode::Off);
    assert_eq!(IconMode::from_setting("generic"), IconMode::Generic);
    assert_eq!(IconMode::from_setting("native"), IconMode::Native);
    assert_eq!(IconMode::from_setting("neon"), IconMode::Native);
    assert_eq!(IconMode::default(), IconMode::Native);
}

#[test]
fn off_shows_no_icon_and_generic_shows_file_or_folder() {
    let mut icons = Icons::default();
    let file = entry("a.txt", EntryKind::File);
    let folder = entry("src", EntryKind::Dir);
    icons.set_mode(IconMode::Off);
    assert_eq!(icons.of(&dir(), Some(&file)), None);
    icons.set_mode(IconMode::Generic);
    assert_eq!(icons.of(&dir(), Some(&file)), Some(icons.file.clone()));
    assert_eq!(icons.of(&dir(), Some(&folder)), Some(icons.folder.clone()));
    assert_eq!(
        icons.of(&dir(), None),
        Some(icons.folder.clone()),
        "the `..` row"
    );
}

#[test]
fn without_native_icons_native_shows_the_generic_ones_and_loads_nothing() {
    let mut icons = Icons {
        unavailable: true,
        ..Icons::default()
    };
    let file = entry("a.txt", EntryKind::File);
    assert_eq!(icons.of(&dir(), Some(&file)), Some(icons.file.clone()));
    assert!(
        icons
            .request([IconKey::Ext("txt".into())].into_iter())
            .is_none()
    );
    assert!(icons.loader.is_none(), "no thread starts");
}

#[test]
fn a_loaded_native_icon_replaces_the_generic_one() {
    let mut icons = native();
    let tool = entry("tool.exe", EntryKind::File);
    let key = IconKey::File(PathBuf::from("/data/work/tool.exe"));
    assert_eq!(icons.of(&dir(), Some(&tool)), Some(icons.file.clone()));
    icons.cache.want(key.clone());
    let own = pixel();
    icons.answered(vec![(key, Loaded::Icon(own.clone()))]);
    assert_eq!(icons.of(&dir(), Some(&tool)), Some(own));
}

#[test]
fn a_change_of_look_redraws_the_generic_icons_and_drops_the_native_ones() {
    let mut icons = native();
    let key = IconKey::Ext("txt".into());
    icons.cache.want(key.clone());
    icons.answered(vec![(key.clone(), Loaded::Icon(pixel()))]);
    let light = icons.file.clone();
    icons.set_dark(true);
    assert_ne!(icons.file, light);
    assert!(icons.cache.get(&key).is_none());
    // The same look again changes nothing.
    let dark = icons.file.clone();
    icons.set_dark(true);
    assert_eq!(icons.file, dark);
}

#[test]
fn leaving_native_drops_the_native_icons() {
    let mut icons = native();
    let key = IconKey::Folder;
    icons.cache.want(key.clone());
    icons.answered(vec![(key.clone(), Loaded::Icon(pixel()))]);
    icons.set_mode(IconMode::Generic);
    assert!(icons.cache.get(&key).is_none());
    assert_eq!(icons.mode, IconMode::Generic);
}

/// RC-080-45: the icons of the rows on screen are asked for once while those rows stay; an
/// answer (which may leave keys to ask again) asks again.
#[tokio::test]
async fn the_rows_icons_are_asked_for_again_only_when_something_changed() {
    let root = tempfile::tempdir().expect("temp dir");
    std::fs::write(root.path().join("a.txt"), b"a").expect("write");
    let mut harness = crate::app::harness::Harness::headless(root.path()).await;
    let app = &mut harness.app;
    // Native icons in reach, nothing asked yet (off Windows the loader has no icons).
    app.icons.unavailable = false;
    app.icons.asked = None;
    let _ = app.request_icons();
    assert!(app.icons.cache.want(IconKey::Folder).is_none(), "asked");

    app.icons.cache.clear();
    let _ = app.request_icons();
    assert!(
        app.icons.cache.want(IconKey::Folder).is_some(),
        "the same rows: not asked again"
    );

    app.icons.answered(Vec::new());
    app.icons.cache.clear();
    let _ = app.request_icons();
    assert!(
        app.icons.cache.want(IconKey::Folder).is_none(),
        "asked again after an answer"
    );
}
