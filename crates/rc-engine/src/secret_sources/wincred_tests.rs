//! The credential manager's blob decoding and target bounds on every platform; on Windows a
//! canary entry written with `cmdkey` is found and read back.

use super::*;

fn utf16(text: &str) -> Vec<u8> {
    text.encode_utf16().flat_map(u16::to_le_bytes).collect()
}

#[test]
fn a_blob_is_read_as_utf16_or_utf8() {
    // Windows' dialog and cmdkey: UTF-16LE, sometimes with a closing NUL.
    assert_eq!(
        decode(&utf16("p\u{e4}sswort")).as_deref(),
        Some("p\u{e4}sswort")
    );
    let mut closed = utf16("abcd");
    closed.extend([0, 0]);
    assert_eq!(decode(&closed).as_deref(), Some("abcd"));
    // Git Credential Manager: UTF-8 - even length, which UTF-16 would misread.
    assert_eq!(decode(b"abcd").as_deref(), Some("abcd"));
    assert_eq!(
        decode("t\u{f6}k\u{20ac}n".as_bytes()).as_deref(),
        Some("t\u{f6}k\u{20ac}n")
    );
    // Neither: an odd length with a NUL, or a lone surrogate.
    assert_eq!(decode(&[b'a', 0, b'b']), None);
    assert_eq!(decode(&[0x00, 0xd8]), None);
    assert_eq!(decode(&[]).as_deref(), Some(""));
}

#[test]
fn a_target_a_reference_cannot_name_is_refused() {
    assert!(usable_target("git:https://example.org"));
    assert!(!usable_target(""));
    assert!(!usable_target("line\nbreak"));
    assert!(!usable_target(&"t".repeat(MAX_ENTRY_ID_CHARS + 1)));
    assert_eq!(
        get("line\nbreak").expect_err("refused").kind,
        FailureKind::BadData
    );
    let long = "q".repeat(MAX_QUERY_CHARS + 1);
    assert_eq!(
        search(&long, 10).expect_err("refused").code(),
        Some(codes::PLUGIN_REQUEST_TOO_LARGE)
    );
}

#[cfg(not(windows))]
#[test]
fn other_platforms_have_no_credential_manager() {
    const { assert!(!PLATFORM) };
    assert_eq!(
        search("", 10).expect_err("none").kind,
        FailureKind::NotSupported
    );
    assert_eq!(
        get("anything").expect_err("none").kind,
        FailureKind::NotSupported
    );
}

/// The canary on Windows: an entry `cmdkey` stores is found by its user and read back, and
/// answers `NotFound` once it is deleted.
#[cfg(windows)]
#[test]
fn a_cmdkey_entry_is_found_and_read() {
    use rc_vault::ExposeSecret as _;

    const TARGET: &str = "rcommander-test/wincred-canary";
    const CANARY: &str = "canary-wincred-4711";
    let cmdkey = |args: &[&str]| {
        // console-window: allow (a test helper; the test runs from a console)
        std::process::Command::new("cmdkey")
            .args(args)
            .output()
            .expect("cmdkey runs")
    };
    let stored = cmdkey(&[
        &format!("/generic:{TARGET}"),
        "/user:rc-canary-user",
        &format!("/pass:{CANARY}"),
    ]);
    assert!(stored.status.success(), "{stored:?}");
    let found = search("rc-canary-user", 50).expect("searched");
    let removed = cmdkey(&[&format!("/delete:{TARGET}")]);
    let read = get(TARGET);
    assert!(removed.status.success(), "{removed:?}");
    assert!(found.iter().any(|entry| entry.id == TARGET), "{found:?}");
    assert!(!format!("{found:?}").contains(CANARY));
    assert_eq!(read.expect("read").expose_secret(), CANARY);
    assert_eq!(get(TARGET).expect_err("gone").kind, FailureKind::NotFound);
}
