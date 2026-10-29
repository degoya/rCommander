use std::path::{Path, PathBuf};

use super::bundled_dir_of;

#[test]
fn the_packages_lie_beside_a_plain_executable() {
    assert_eq!(
        bundled_dir_of(Path::new("/opt/rcommander/rcommander")),
        Some(PathBuf::from("/opt/rcommander/plugins"))
    );
}

#[test]
fn an_app_bundle_keeps_them_in_its_resources() {
    assert_eq!(
        bundled_dir_of(Path::new(
            "/Applications/rCommander.app/Contents/MacOS/rcommander"
        )),
        Some(PathBuf::from(
            "/Applications/rCommander.app/Contents/Resources/plugins"
        ))
    );
}

#[test]
fn a_folder_named_macos_outside_a_bundle_is_a_plain_folder() {
    assert_eq!(
        bundled_dir_of(Path::new("/home/me/MacOS/rcommander")),
        Some(PathBuf::from("/home/me/MacOS/plugins"))
    );
}
