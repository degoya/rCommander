//! The parsers against fixtures of real server answers (`tests/fixtures/ftp_list/`).

use std::{
    path::PathBuf,
    time::{Duration, SystemTime},
};

use rc_vfs::{Entry, EntryKind};

use super::{Dialect, parse_facts, parse_line};
use crate::ftp::encoding::Wire;

const UNIX: &str = include_str!("../../tests/fixtures/ftp_list/unix.txt");
const DOS: &str = include_str!("../../tests/fixtures/ftp_list/dos.txt");
const MLSD: &str = include_str!("../../tests/fixtures/ftp_list/mlsd.txt");
const UNIX_LATIN1: &[u8] = include_bytes!("../../tests/fixtures/ftp_list/unix-latin1.txt");

/// 2026-10-17 12:00 UTC, the "now" every year inference here is relative to.
fn now() -> SystemTime {
    at(1_792_238_400)
}

fn at(seconds: u64) -> SystemTime {
    SystemTime::UNIX_EPOCH + Duration::from_secs(seconds)
}

fn parse_all(text: &str, dialect: Dialect) -> Vec<Entry> {
    text.lines()
        .filter_map(|line| parse_line(line, dialect, now()))
        .collect()
}

fn find<'a>(entries: &'a [Entry], name: &str) -> &'a Entry {
    entries
        .iter()
        .find(|entry| entry.name == name)
        .unwrap_or_else(|| panic!("{name} missing from {entries:#?}"))
}

#[test]
fn unix_lines_parse() {
    let entries = parse_all(UNIX, Dialect::List);
    let names: Vec<&str> = entries.iter().map(|entry| entry.name.as_str()).collect();
    assert_eq!(
        names,
        [
            "public_html",
            ".bash_logout",
            "backup 2025.tar",
            "www",
            "acl file.txt",
            "readonly-no-group.txt",
            "tmp",
            "null"
        ]
    );

    let dir = find(&entries, "public_html");
    assert_eq!(dir.kind, EntryKind::Dir);
    assert_eq!(dir.size, 0, "a directory has size 0");
    assert_eq!(dir.attributes.mode, Some(0o755));
    assert_eq!(
        dir.modified,
        Some(at(1_768_046_400)),
        "this year's Jan 10 12:00"
    );

    let hidden = find(&entries, ".bash_logout");
    assert!(hidden.hidden);
    assert_eq!(
        hidden.modified,
        Some(at(1_551_571_200)),
        "a year instead of a time"
    );

    let big = find(&entries, "backup 2025.tar");
    assert_eq!(big.size, 5_368_709_120, "a size above 4 GiB");
    assert_eq!(big.kind, EntryKind::File);

    assert_eq!(
        find(&entries, "www").kind,
        EntryKind::Symlink {
            target: PathBuf::from("public_html"),
            target_kind: None
        }
    );
    // Dec 31 23:59 would be in the future this year, so it is last year's.
    assert_eq!(
        find(&entries, "acl file.txt").modified,
        Some(at(1_767_225_540))
    );
    let readonly = find(&entries, "readonly-no-group.txt");
    assert_eq!(readonly.size, 1234);
    assert!(readonly.attributes.readonly);
    assert_eq!(find(&entries, "tmp").attributes.mode, Some(0o1777));
    assert_eq!(find(&entries, "null").kind, EntryKind::Other);
}

#[test]
fn dos_lines_parse() {
    let entries = parse_all(DOS, Dialect::List);
    let names: Vec<&str> = entries.iter().map(|entry| entry.name.as_str()).collect();
    assert_eq!(
        names,
        [
            "extras",
            "readme.txt",
            "big file.iso",
            "new folder",
            "report.pdf"
        ]
    );
    assert_eq!(find(&entries, "extras").kind, EntryKind::Dir);
    assert_eq!(find(&entries, "readme.txt").size, 403);
    let big = find(&entries, "big file.iso");
    assert_eq!(big.size, 5_368_709_120);
    assert_eq!(big.modified, Some(at(1_603_066_740)), "12:19AM is 00:19");
    assert_eq!(
        find(&entries, "new folder").modified,
        Some(at(1_768_050_300)),
        "a four-digit year and 01:05PM"
    );
    let report = find(&entries, "report.pdf");
    assert_eq!(report.size, 1234, "thousands separators");
    assert_eq!(
        report.modified,
        Some(at(1_768_053_900)),
        "ISO date, 24 hours"
    );
}

#[test]
fn mlsd_lines_parse() {
    let entries = parse_all(MLSD, Dialect::Mlsd);
    let names: Vec<&str> = entries.iter().map(|entry| entry.name.as_str()).collect();
    assert_eq!(
        names,
        [
            "public_html",
            "backup 2025.tar",
            "read only.txt",
            "www",
            "semi;colon.txt"
        ],
        "cdir and pdir name no entry"
    );
    let dir = find(&entries, "public_html");
    assert_eq!(dir.kind, EntryKind::Dir);
    assert_eq!(dir.attributes.mode, Some(0o755));
    let big = find(&entries, "backup 2025.tar");
    assert_eq!(big.size, 5_368_709_120);
    assert_eq!(big.modified, Some(at(1_751_356_800)));
    let readonly = find(&entries, "read only.txt");
    assert!(readonly.attributes.readonly, "perm without w");
    assert_eq!(
        readonly.modified,
        Some(at(1_768_046_400)),
        "fractions dropped"
    );
    assert_eq!(
        find(&entries, "www").kind,
        EntryKind::Symlink {
            target: PathBuf::from("/var/www"),
            target_kind: None
        }
    );
}

#[test]
fn a_list_line_answered_to_mlsd_is_read_as_list() {
    let line = "-rw-r--r-- 1 owner group 1234 Jan 10 12:00 movie.mkv";
    let entry = parse_line(line, Dialect::Mlsd, now()).expect("read as LIST");
    assert_eq!(entry.name, "movie.mkv");
    assert_eq!(entry.size, 1234);
}

#[test]
fn lines_without_an_entry_are_skipped() {
    for line in ["", "   ", "total 12", "garbage that is no listing", "\r\n"] {
        assert_eq!(parse_line(line, Dialect::List, now()), None, "{line:?}");
    }
    let crlf = "drwxr-xr-x 2 a b 4096 Jan 10 12:00 dir\r\n";
    assert_eq!(
        parse_line(crlf, Dialect::List, now()).map(|entry| entry.name),
        Some("dir".to_owned())
    );
}

#[test]
fn an_mlst_answer_parses_with_its_path() {
    let entry =
        parse_facts(" type=file;size=1024;modify=20260110120000; /dir/b.bin").expect("MLST line");
    assert_eq!(entry.name, "/dir/b.bin");
    assert_eq!(entry.size, 1024);
}

#[test]
fn latin1_names_decode_before_parsing() {
    let line = Wire::Latin1.decode(UNIX_LATIN1.trim_ascii_end());
    let entry = parse_line(&line, Dialect::List, now()).expect("entry");
    assert_eq!(entry.name, "Gr\u{fc}\u{df}e.txt");
    assert_eq!(entry.size, 42);
}

#[test]
fn a_listed_name_never_leaves_its_directory() {
    for line in [
        "type=file;size=1; ../../x",
        "type=file;size=1; a/b",
        "type=dir; ..",
        "type=file;size=1; c:\\windows",
        "type=file;size=1; nul\0byte",
        "type=file;size=1; cr\rinside",
        "-rw-r--r-- 1 owner group 12 Jan 10 12:00 ../escape",
        "-rw-r--r-- 1 owner group 12 Jan 10 12:00 dir/file",
        "01-10-26  12:00PM                   12 ..\\up.txt",
        "type=file;size=1; C:evil.exe",
        "type=file;size=1; d:",
        "type=file;size=1; tab\tinside",
        "type=file;size=1; escape\u{1b}[2J",
        "-rw-r--r-- 1 owner group 12 Jan 10 12:00 Z:autoexec.bat",
    ] {
        assert!(
            parse_line(line, Dialect::Mlsd, now()).is_none(),
            "{line:?} was believed"
        );
    }
    for fine in ["fine name.txt", "notes: draft.txt", "1:2"] {
        let plain = parse_line(&format!("type=file;size=1; {fine}"), Dialect::Mlsd, now());
        assert_eq!(plain.map(|entry| entry.name).as_deref(), Some(fine));
    }
}
