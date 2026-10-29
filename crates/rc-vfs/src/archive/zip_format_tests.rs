use std::{path::Path, time::SystemTime};

use chrono::{Local, TimeZone};

use super::dos_time;
use crate::{
    Vfs,
    archive::{
        ArchiveSettings,
        test_support::{code, list, mtime, names, open, open_with, read},
    },
};

/// Acceptance: a name stored in CP437 without the UTF-8 flag shows its umlauts.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cp437_names_show_their_umlauts() {
    let fs = open("cp437.zip").await;
    let name = "Gr\u{f6}\u{df}e.txt";
    assert_eq!(names(&list(&fs, "/").await), [name]);
    assert_eq!(read(&fs, &format!("/{name}")).await, b"legacy name\n");
}

/// Another OEM code page is a setting; a name flagged UTF-8 ignores it.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn another_code_page_is_a_setting() {
    let cyrillic = ArchiveSettings {
        zip_legacy_encoding: "cp866".to_owned(),
        ..ArchiveSettings::default()
    };
    let fs = open_with("cp866.zip", cyrillic.clone()).await;
    assert_eq!(
        names(&list(&fs, "/").await),
        ["\u{41f}\u{440}\u{438}\u{432}\u{435}\u{442}.txt"]
    );
    let flagged = open_with("tree.zip", cyrillic).await;
    assert_eq!(
        names(&list(&flagged, "/unicode").await),
        ["Gr\u{fc}\u{df}e.txt"]
    );

    let unknown = ArchiveSettings {
        zip_legacy_encoding: "no-such-code-page".to_owned(),
        ..ArchiveSettings::default()
    };
    assert_eq!(unknown.legacy_encoding(), None, "an unknown label is CP437");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn zip64_is_read() {
    let fs = open("zip64.zip").await;
    let entries = list(&fs, "/").await;
    assert_eq!(names(&entries), ["zip64.txt"]);
    assert_eq!((entries[0].size, entries[0].modified), (12, Some(mtime())));
    assert_eq!(read(&fs, "/zip64.txt").await, b"zip64 entry\n");
}

/// Acceptance: encrypted entries, ZipCrypto and AES, are marked and need a password.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn encrypted_entries_are_marked_and_need_a_password() {
    for name in ["encrypted-aes.zip", "encrypted-zipcrypto.zip"] {
        let fs = open(name).await;
        let item = fs
            .stat_detailed(Path::new("/secret.txt"))
            .await
            .expect(name);
        assert!(item.info.encrypted, "{name}");
        assert_eq!(
            code(fs.open_read(Path::new("/secret.txt")).await),
            "password_required",
            "{name}"
        );
    }
}

/// A zip names the time of each entry in the packer's local time, unless an extra field says
/// UTC; the fixtures carry one, this is the fallback.
#[test]
fn a_dos_time_is_local_time() {
    let stamp = zip::DateTime::from_date_and_time(2024, 5, 17, 12, 34, 56).expect("valid");
    let local = Local
        .with_ymd_and_hms(2024, 5, 17, 12, 34, 56)
        .earliest()
        .expect("a local time");
    assert_eq!(dos_time(stamp), Some(SystemTime::from(local)));
}
