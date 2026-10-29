use super::{ExtractSettings, folder_name};

#[test]
fn the_own_folder_drops_the_archive_endings() {
    assert_eq!(folder_name("photos.zip"), "photos");
    assert_eq!(folder_name("site.tar.gz"), "site");
    assert_eq!(folder_name("Backup.TAR.ZST"), "Backup");
    assert_eq!(folder_name("v1.2.7z"), "v1.2");
    assert_eq!(folder_name("readme.txt.gz"), "readme.txt");
    // Nothing but an ending: the name stays.
    assert_eq!(folder_name(".zip"), ".zip");
}

#[test]
fn the_defaults_are_total_commanders() {
    let settings = ExtractSettings::default();
    assert!(!settings.own_folder);
    assert!(settings.keep_paths);
    assert_eq!(settings.mask, "*.*");
}
