//! The fixtures in `tests/fixtures/wsl/` hold the bytes `wsl.exe` writes into a pipe, one file
//! per form: `list-q-*` for `--list --quiet`, `list-v-*` for `--list --verbose`. They are built
//! byte for byte after the known forms (the sandbox these tests were written in cannot start
//! Windows programs); a capture from a real Windows run replaces a file of the same name.

use std::path::PathBuf;

use super::*;

fn fixture(name: &str) -> Vec<u8> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/wsl")
        .join(name);
    std::fs::read(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()))
}

fn names_in(name: &str) -> Vec<String> {
    names(&decode(&fixture(name)))
}

#[test]
fn utf16le_with_a_byte_order_mark() {
    assert_eq!(names_in("list-q-utf16-bom.bin"), ["Ubuntu", "Debian"]);
}

#[test]
fn utf16le_without_a_byte_order_mark() {
    assert_eq!(
        names_in("list-q-utf16.bin"),
        ["Ubuntu-24.04", "docker-desktop"]
    );
}

#[test]
fn nul_padding_and_blank_lines_are_dropped() {
    assert_eq!(names_in("list-q-utf16-nul.bin"), ["Ubuntu", "kali-linux"]);
}

#[test]
fn crlf_and_lf_line_ends_read_alike() {
    assert_eq!(
        names_in("list-q-utf16-lf.bin"),
        ["Alpine", "openSUSE-Tumbleweed"]
    );
    assert!(
        !names_in("list-q-utf16.bin")
            .iter()
            .any(|name| name.contains('\r'))
    );
}

#[test]
fn utf8_output_with_wsl_utf8() {
    assert_eq!(names_in("list-q-utf8.bin"), ["Ubuntu", "Debian"]);
}

#[test]
fn empty_output_lists_nothing() {
    assert!(names_in("list-q-empty.bin").is_empty());
}

#[test]
fn the_messages_instead_of_a_list_are_no_names() {
    assert!(names_in("list-q-no-distro.bin").is_empty());
    assert!(names_in("list-q-not-installed.bin").is_empty());
}

#[test]
fn utf16le_is_recognised_without_a_mark() {
    let bytes: Vec<u8> = "Ubuntu\r\n"
        .encode_utf16()
        .flat_map(u16::to_le_bytes)
        .collect();
    assert_eq!(decode(&bytes), "Ubuntu\r\n");
    assert_eq!(decode(b"Ubuntu\r\n"), "Ubuntu\r\n");
    assert_eq!(decode(b"\xEF\xBB\xBFUbuntu"), "Ubuntu");
}

#[test]
fn a_trailing_odd_byte_and_a_lone_surrogate_do_not_panic() {
    let bytes = [0xFF, 0xFE, b'A', 0, 0x00, 0xD8, b'B', 0, b'C'];
    assert_eq!(decode(&bytes), "A\u{FFFD}B");
}

#[test]
fn a_name_is_listed_once_whatever_its_case() {
    assert_eq!(names("Ubuntu\nubuntu\nDebian\n"), ["Ubuntu", "Debian"]);
}

#[test]
fn only_what_wsl_accepts_as_a_name_is_a_name() {
    assert_eq!(
        names("Ubuntu 24.04\nhttps://aka.ms/wsl\n..\nUbuntu_dev.1\n"),
        ["Ubuntu_dev.1"]
    );
}

#[test]
fn verbose_marks_the_default_and_reads_the_versions() {
    let details = verbose(&decode(&fixture("list-v-utf16.bin")));
    assert_eq!(details.default.as_deref(), Some("Ubuntu"));
    assert_eq!(
        details.versions,
        [
            ("Ubuntu".to_owned(), 2),
            ("Debian".to_owned(), 1),
            ("docker-desktop".to_owned(), 2)
        ]
    );
}

#[test]
fn verbose_reads_a_translated_table_with_spaces_in_the_state() {
    let details = verbose(&decode(&fixture("list-v-de-utf16.bin")));
    assert_eq!(details.default.as_deref(), Some("Ubuntu-24.04"));
    assert_eq!(
        details.versions,
        [("Ubuntu".to_owned(), 2), ("Ubuntu-24.04".to_owned(), 2)]
    );
}

#[test]
fn verbose_without_a_marked_row_has_no_default() {
    let details = verbose("  NAME STATE VERSION\n  Debian Stopped 1\n");
    assert_eq!(details.default, None);
    assert_eq!(details.versions, [("Debian".to_owned(), 1)]);
    assert_eq!(verbose(""), Details::default());
}
