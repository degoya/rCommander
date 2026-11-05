use std::path::{Path, PathBuf};

use super::{CopyLinks, MAX_LINK_DEPTH, Trail, normal};

#[test]
fn the_setting_names_each_way_and_falls_back_to_contents() {
    assert_eq!(CopyLinks::from_setting("contents"), CopyLinks::Contents);
    assert_eq!(CopyLinks::from_setting("link"), CopyLinks::Link);
    assert_eq!(CopyLinks::from_setting("skip"), CopyLinks::Skip);
    assert_eq!(CopyLinks::from_setting("other"), CopyLinks::Contents);
    assert!(CopyLinks::default().is_default());
}

#[test]
fn a_path_is_made_plain_without_the_file_system() {
    assert_eq!(normal(Path::new("/a/./b/../c")), PathBuf::from("/a/c"));
    assert_eq!(normal(Path::new("/../x")), PathBuf::from("/x"));
}

#[test]
fn a_link_to_a_folder_elsewhere_is_followed() {
    let into = Trail::default()
        .follow(Path::new("/a/l"), Path::new("../e"), false)
        .expect("not on the way");
    assert!(into.below());
    assert_eq!(into.child("x").real, Some(PathBuf::from("/e/x")));
}

#[test]
fn a_link_to_a_folder_above_it_is_a_ring() {
    let plain = Trail::default();
    assert!(
        plain
            .follow(Path::new("/a/b/l"), Path::new(".."), false)
            .is_none()
    );
    assert!(
        plain
            .follow(Path::new("/a/l"), Path::new("/"), false)
            .is_none()
    );
    assert!(
        plain
            .follow(Path::new("/a/l"), Path::new("."), false)
            .is_none()
    );
}

#[test]
fn a_source_that_is_a_link_is_followed_once_and_not_again() {
    // `/a/l` leads to `/a`, and is copied itself: `/a` is walked once, and `/a/l` inside it is
    // the ring.
    let into = Trail::default()
        .follow(Path::new("/a/l"), Path::new("/a"), true)
        .expect("a source is followed");
    let inner = into.child("l");
    assert!(
        inner
            .follow(Path::new("/a/l/l"), Path::new("/a"), false)
            .is_none()
    );
}

#[test]
fn two_links_that_lead_into_each_other_are_a_ring() {
    // `/a/x` leads to `/e`, `/e/y` back to `/a`.
    let into = Trail::default()
        .follow(Path::new("/a/x"), Path::new("/e"), false)
        .expect("elsewhere");
    let y = into.child("y");
    assert!(
        y.follow(Path::new("/a/x/y"), Path::new("/a"), false)
            .is_none()
    );
}

#[test]
fn links_too_deep_are_not_followed() {
    let mut trail = Trail::default();
    let mut path = PathBuf::from("/r");
    for depth in 0..MAX_LINK_DEPTH {
        let link = path.join(format!("l{depth}"));
        trail = trail
            .follow(&link, &PathBuf::from(format!("/t{depth}")), false)
            .expect("still shallow");
        path = link;
    }
    assert!(
        trail
            .follow(&path.join("deep"), Path::new("/deep"), false)
            .is_none()
    );
}
