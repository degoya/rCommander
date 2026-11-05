//! The fuzz targets' way into the listing parser (`parse_listing`, RC-080-72) reads a listing as
//! the backend does.

use std::time::{Duration, SystemTime};

use rc_vfs::EntryKind;

use super::{NameEncoding, parse_listing};

fn now() -> SystemTime {
    SystemTime::UNIX_EPOCH + Duration::from_secs(1_792_238_400)
}

#[test]
fn a_list_answer_is_split_decoded_and_judged() {
    let listing = b"total 8\r\n\
        -rw-r--r--    1 ftp      ftp           220 Mar 03  2019 caf\xe9.txt\r\n\
        drwxr-xr-x    2 ftp      ftp          4096 Mar 03  2019 ..\r\n";
    let entries = parse_listing(listing, false, NameEncoding::Latin1, now());
    let names: Vec<&str> = entries.iter().map(|entry| entry.name.as_str()).collect();
    assert_eq!(names, ["caf\u{e9}.txt"]);
    assert_eq!(entries[0].kind, EntryKind::File);
    assert_eq!(entries[0].size, 220);
}

#[test]
fn an_mlsd_answer_reads_its_facts() {
    let listing = b"type=dir;modify=20260110120000; docs\r\ntype=file;size=7; a.txt\r\n";
    let entries = parse_listing(listing, true, NameEncoding::Utf8, now());
    let kinds: Vec<(&str, EntryKind)> = entries
        .iter()
        .map(|entry| (entry.name.as_str(), entry.kind.clone()))
        .collect();
    assert_eq!(
        kinds,
        [("docs", EntryKind::Dir), ("a.txt", EntryKind::File)]
    );
}
