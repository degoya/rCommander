use std::path::Path;

use rc_vfs::BackendId;

use super::{check_listed, check_name, check_target, keeps_windows_rules};

#[test]
fn plain_names_pass() {
    let longest = "x".repeat(255);
    for name in [
        "report.pdf",
        "a b",
        ".hidden",
        "con-tract.txt",
        "console",
        &longest,
    ] {
        assert!(check_name(name).is_ok(), "{name:?}");
    }
}

#[test]
fn paths_and_names_windows_refuses_are_invalid() {
    let long = "x".repeat(256);
    for name in [
        "",
        ".",
        "..",
        "../escape",
        "a/b",
        "a\\b",
        "c:file",
        "what?",
        "pipe|name",
        "tab\there",
        "nul\u{0}byte",
        "trailing.",
        "trailing ",
        "CON",
        "con.txt",
        "Lpt1",
        "nul.tar.gz",
        "aux .txt",
        "COM\u{b9}",
        "invoice\u{202e}fdp.exe",
        "left\u{2067}to",
        long.as_str(),
    ] {
        let refused = check_name(name).expect_err(name);
        assert_eq!(refused.code(), "invalid_name", "{name:?}");
    }
}

/// A listed name a join would turn into a way out of its directory is refused; on Windows also
/// a backslash or a colon, which Windows reads as a separator, a drive or a stream.
#[test]
fn a_listed_name_must_be_one_plain_name() {
    for name in ["report.pdf", ".hidden", "a b", "x*?.txt"] {
        assert!(check_listed(name).is_ok(), "{name}");
    }
    for name in ["", ".", "..", "../x", "/abs", "a/b", "a/", "./a"] {
        let error = check_listed(name).expect_err(name);
        assert_eq!(error.code(), "invalid_name", "{name}");
    }
    for name in ["a\\b", "..\\x", "c:x", "file:stream"] {
        assert_eq!(check_listed(name).is_err(), cfg!(windows), "{name}");
    }
}

/// RC-080-12 (X2): every name a copy gives a local Windows folder - the source's own first, a
/// synchronization's rows too - keeps Windows' rules; elsewhere names stay as the source holds
/// them.
#[test]
fn a_copy_to_a_windows_folder_keeps_windows_rules_for_every_name() {
    for rel in [
        "notes: draft.txt",
        "con",
        "report.",
        "invoice\u{202e}fdp.exe",
        "dir/a:b",
        "ok/nul.txt",
    ] {
        let refused = check_target(Path::new(rel), true).expect_err(rel);
        assert_eq!(refused.code(), "invalid_name", "{rel}");
        assert!(check_target(Path::new(rel), false).is_ok(), "{rel}");
    }
    assert!(check_target(Path::new("dir/report.pdf"), true).is_ok());
    assert_eq!(keeps_windows_rules(&BackendId::Local), cfg!(windows));
    assert!(!keeps_windows_rules(&BackendId::Memory(1)));
}
