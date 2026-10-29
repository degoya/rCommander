use std::path::{Path, PathBuf};

use super::*;

#[test]
fn an_ftp_location_is_written_with_its_connection_name() {
    assert_eq!(ftp_location("nas", Path::new("/")), "ftp://nas/");
    assert_eq!(
        ftp_location("nas", Path::new("/pub/My Files")),
        "ftp://nas/pub/My Files"
    );
}

#[test]
fn a_location_splits_into_name_and_folded_path() {
    assert_eq!(
        split_location("nas/pub/./a/../b"),
        ("nas", Some(PathBuf::from("/pub/b")))
    );
    assert_eq!(split_location("nas/"), ("nas", Some(PathBuf::from("/"))));
    assert_eq!(split_location("nas"), ("nas", None));
    assert_eq!(
        split_location("My NAS//x"),
        ("My NAS", Some(PathBuf::from("/x")))
    );
}

#[test]
fn dot_dot_stops_at_the_root() {
    assert_eq!(fold("../../etc"), PathBuf::from("/etc"));
    assert_eq!(fold("/srv/www/"), PathBuf::from("/srv/www"));
    assert_eq!(fold(""), PathBuf::from("/"));
}

#[test]
fn the_scheme_is_matched_ignoring_case() {
    assert_eq!(strip_ftp_scheme("ftp://nas/pub"), Some("nas/pub"));
    assert_eq!(strip_ftp_scheme("FTP://nas"), Some("nas"));
    // `ftps://` names an FTP connection too (RC-070-03).
    assert_eq!(strip_ftp_scheme("ftps://nas"), Some("nas"));
    assert_eq!(strip_ftp_scheme("FTPS://nas/pub"), Some("nas/pub"));
    assert_eq!(strip_ftp_scheme("sftp://nas"), None);
    assert_eq!(strip_ftp_scheme("/home"), None);
    assert_eq!(strip_ftp_scheme("ftp:"), None);
}
