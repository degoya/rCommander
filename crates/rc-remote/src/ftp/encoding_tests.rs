use std::path::Path;

use super::{Wire, ftp_path};
use crate::ftp::config::NameEncoding;

#[test]
fn utf8_wins_when_the_server_offers_it() {
    assert_eq!(Wire::negotiate(true, NameEncoding::Latin1), Wire::Utf8);
    assert_eq!(Wire::negotiate(false, NameEncoding::Latin1), Wire::Latin1);
    assert_eq!(Wire::negotiate(false, NameEncoding::Utf8), Wire::Utf8);
}

#[test]
fn latin1_bytes_decode_to_umlauts() {
    // "Gr\xFC\xDFe.txt" is "Gruesse" with u-umlaut and sharp s in ISO 8859-1.
    let bytes = b"Gr\xfc\xdfe.txt";
    assert_eq!(Wire::Latin1.decode(bytes), "Gr\u{fc}\u{df}e.txt");
    assert_eq!(Wire::Utf8.decode(bytes), "Gr\u{fffd}\u{fffd}e.txt");
    assert_eq!(
        Wire::Utf8.decode("Gr\u{fc}\u{df}e.txt".as_bytes()),
        "Gr\u{fc}\u{df}e.txt"
    );
}

#[test]
fn paths_become_absolute_ftp_paths() {
    let path = |text: &str| ftp_path(Path::new(text), Wire::Utf8);
    assert_eq!(path("/").expect("root"), "/");
    assert_eq!(path("/a/b c/d.txt").expect("plain"), "/a/b c/d.txt");
    assert_eq!(path("/a/./b/").expect("dot"), "/a/b");
    // Joined with the platform's separator, a backslash on Windows.
    let joined = Path::new("/x").join("y");
    assert_eq!(ftp_path(&joined, Wire::Utf8).expect("joined"), "/x/y");
}

#[test]
fn unsafe_paths_are_refused() {
    let code = |text: &str, wire| {
        ftp_path(Path::new(text), wire)
            .map_err(|error| error.code())
            .err()
    };
    assert_eq!(code("relative", Wire::Utf8), Some("invalid_path"));
    assert_eq!(code("/a/../etc", Wire::Utf8), Some("invalid_path"));
    assert_eq!(code("/a\r\nDELE b", Wire::Utf8), Some("invalid_path"));
    assert_eq!(code("/nul\0", Wire::Utf8), Some("invalid_path"));
    assert_eq!(code("/Gr\u{fc}\u{df}e", Wire::Latin1), Some("unsupported"));
    assert_eq!(code("/Gr\u{fc}\u{df}e", Wire::Utf8), None);
}
