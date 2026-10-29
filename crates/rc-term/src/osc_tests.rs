use super::{MAX_PAYLOAD, OscEvent, OscScanner, PromptMark, parse_osc};

fn payloads(chunks: &[&[u8]]) -> Vec<Vec<u8>> {
    let mut scanner = OscScanner::default();
    let mut found = Vec::new();
    for chunk in chunks {
        scanner.feed(chunk, |payload| found.push(payload.to_vec()));
    }
    found
}

#[test]
fn payloads_end_with_bel_or_st_also_across_chunks() {
    assert_eq!(
        payloads(&[b"ab\x1b]7;file:///x\x07cd\x1b]133;A\x1b\\"]),
        [b"7;file:///x".to_vec(), b"133;A".to_vec()]
    );
    assert_eq!(
        payloads(&[b"\x1b]13", b"3;B\x1b", b"\\rest"]),
        [b"133;B".to_vec()]
    );
}

#[test]
fn other_sequences_and_aborted_ones_yield_nothing() {
    assert!(payloads(&[b"\x1b[31mred\x1b[0m \x1bP1$r\x1b\\"]).is_empty());
    assert!(payloads(&[b"\x1b]7;abc\x18\x07"]).is_empty(), "CAN aborts");
    // An ESC that is not ST aborts; a new OSC right after it still counts.
    assert_eq!(payloads(&[b"\x1b]7;x\x1b]133;A\x07"]), [b"133;A".to_vec()]);
}

#[test]
fn an_overlong_payload_is_dropped_and_the_next_one_counts() {
    let mut long = b"\x1b]7;".to_vec();
    long.extend(std::iter::repeat_n(b'a', MAX_PAYLOAD + 10));
    long.extend(b"\x07\x1b]133;C\x07");
    assert_eq!(payloads(&[&long]), [b"133;C".to_vec()]);
}

fn folder(payload: &str) -> Option<String> {
    match parse_osc(payload.as_bytes(), &["Desk-1".to_owned()]) {
        Some(OscEvent::Folder(path)) => Some(path),
        Some(other) => panic!("a folder, got {other:?}"),
        None => None,
    }
}

#[test]
fn osc7_urls_of_this_machine_become_paths() {
    let cases: &[(&str, Option<&str>)] = &[
        ("7;file:///home/x", Some("/home/x")),
        ("7;file://localhost/home/x", Some("/home/x")),
        ("7;FILE://desk-1/home/x", Some("/home/x")),
        ("7;file://desk-1/home/my%20dir", Some("/home/my dir")),
        ("7;file://desk-1/home/%C3%A4%27%24", Some("/home/\u{e4}'$")),
        ("7;file://desk-1/C:/Users/x", Some("/C:/Users/x")),
        ("7;file:///a%3Fb/c?query#frag", Some("/a?b/c")),
        // Another host: an SSH session inside the shell.
        ("7;file://server/home/x", None),
        // Not a file URL, no path, broken or hostile escapes.
        ("7;http://localhost/x", None),
        ("7;file://localhost", None),
        ("7;file:///a%2", None),
        ("7;file:///a%zz", None),
        ("7;file:///a%0Ab", None),
        ("7;file:///a%FF", None),
    ];
    for (payload, expected) in cases {
        assert_eq!(folder(payload).as_deref(), *expected, "{payload}");
    }
}

#[test]
fn osc9_9_takes_a_plain_path_quoted_or_not() {
    assert_eq!(
        folder("9;9;\"C:\\Users\\x\"").as_deref(),
        Some("C:\\Users\\x")
    );
    assert_eq!(folder("9;9;C:\\a b").as_deref(), Some("C:\\a b"));
    assert_eq!(folder("9;9;").as_deref(), None);
    assert_eq!(folder("9;9;a\tb").as_deref(), None);
}

#[test]
fn a_network_share_is_never_a_folder_but_a_wsl_share_is() {
    // A server's output can name any of these; Windows would sign in to `attacker` (RC-080-10).
    for payload in [
        r"9;9;\\attacker\share",
        r#"9;9;"\\attacker\share\x""#,
        "9;9;//attacker/share",
        r"9;9;\\?\UNC\attacker\share",
        r"9;9;\??\UNC\attacker\share",
        r"9;9;\\.\pipe\x",
        "7;file:////attacker/share",
        "7;file://localhost//attacker/share/x",
        "7;file:///%5C%5Cattacker%5Cshare",
        "7;file://attacker/share",
    ] {
        assert_eq!(folder(payload), None, "{payload}");
    }
    for (payload, expected) in [
        (
            r"9;9;\\wsl.localhost\Ubuntu\home\x",
            r"\\wsl.localhost\Ubuntu\home\x",
        ),
        (r"9;9;\\wsl$\Debian", r"\\wsl$\Debian"),
        (
            "7;file://localhost//wsl.localhost/Ubuntu/home",
            "//wsl.localhost/Ubuntu/home",
        ),
    ] {
        assert_eq!(folder(payload).as_deref(), Some(expected), "{payload}");
    }
}

#[test]
fn osc133_marks_are_read_and_others_ignored() {
    let mark = |text: &str| parse_osc(text.as_bytes(), &[]);
    assert_eq!(
        mark("133;A"),
        Some(OscEvent::Prompt(PromptMark::PromptStart))
    );
    assert_eq!(
        mark("133;B"),
        Some(OscEvent::Prompt(PromptMark::InputStart))
    );
    assert_eq!(
        mark("133;C"),
        Some(OscEvent::Prompt(PromptMark::CommandStart))
    );
    assert_eq!(
        mark("133;D;0"),
        Some(OscEvent::Prompt(PromptMark::CommandEnd))
    );
    assert_eq!(mark("133;Z"), None);
    assert_eq!(mark("0;title"), None);
    assert_eq!(parse_osc(b"7;file:///\xff", &[]), None);
}
