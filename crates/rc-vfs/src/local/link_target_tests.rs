use super::judged;

/// Drive `Z` is mapped to a share, every other letter is local.
fn z_is_remote(letter: char) -> bool {
    letter.eq_ignore_ascii_case(&'z')
}

#[test]
fn a_link_onto_a_share_is_not_followed() {
    assert!(judged(r"\\server\share\folder", true, z_is_remote));
    assert!(judged(r"\\?\UNC\server\share\folder", true, z_is_remote));
}

#[test]
fn a_link_onto_a_network_drive_is_not_followed() {
    assert!(judged(r"Z:\projects", true, z_is_remote));
    assert!(judged(r"z:\projects", true, z_is_remote));
    assert!(judged(r"\\?\Z:\projects", true, z_is_remote));
    assert!(judged("Z:", true, z_is_remote));
}

#[test]
fn a_link_onto_a_local_drive_or_folder_is_followed() {
    assert!(!judged(r"C:\Users", true, z_is_remote));
    assert!(!judged(r"\\?\C:\Users", true, z_is_remote));
    assert!(!judged(r"..\sibling", true, z_is_remote));
    assert!(!judged("target.txt", true, z_is_remote));
    assert!(!judged(r"\\wsl.localhost\Ubuntu\home", true, z_is_remote));
}

#[test]
fn only_windows_leaves_a_target_unfollowed() {
    assert!(!judged(r"\\server\share", false, z_is_remote));
    assert!(!judged(r"Z:\projects", false, z_is_remote));
    assert!(!judged("/mnt/z", false, z_is_remote));
}
