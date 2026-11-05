use super::{
    MAX_CLIPBOARD_SEQUENCE, MAX_PAYLOAD, MAX_SEQUENCE, OscEvent, OscScanner, PromptMark, parse_osc,
};

/// The payloads found in `chunks`, and what the parser was given of them.
fn scan(chunks: &[&[u8]]) -> (Vec<Vec<u8>>, Vec<u8>) {
    let mut scanner = OscScanner::default();
    let mut found = Vec::new();
    let mut forward = Vec::new();
    for chunk in chunks {
        scanner.feed(chunk, &mut forward, |payload| found.push(payload.to_vec()));
    }
    (found, forward)
}

fn payloads(chunks: &[&[u8]]) -> Vec<Vec<u8>> {
    scan(chunks).0
}

#[test]
fn the_parser_sees_every_byte_of_a_stream_without_an_overlong_sequence() {
    let stream: &[u8] = b"ab\x1b]0;title\x07\x1b[31mred\x1b]7;file:///x\x1b\\\x1b\x07\x1b]2;t\x18\
        \x1b\x1b]133;A\x1b]133;B\x07\x1bP1$r\x1b\\end\x1b";
    // Every split into two chunks gives the parser the same bytes, in order.
    for cut in 0..=stream.len() {
        let (head, tail) = stream.split_at(cut);
        assert_eq!(scan(&[head, tail]).1, stream, "cut at {cut}");
    }
}

#[test]
fn an_open_sequence_is_held_back_until_it_ends() {
    let mut scanner = OscScanner::default();
    let mut forward = Vec::new();
    scanner.feed(b"x\x1b]0;ti", &mut forward, |_| {});
    assert_eq!(forward, b"x\x1b");
    scanner.feed(b"tle\x07y", &mut forward, |_| {});
    assert_eq!(forward, b"x\x1b]0;title\x07y");
}

#[test]
fn an_overlong_sequence_never_reaches_the_parser_and_memory_stays_bounded() {
    // A 10 MiB OSC in 16 KiB chunks, the way the PTY reader hands it over.
    let mut scanner = OscScanner::default();
    let mut forward = Vec::new();
    scanner.feed(b"a\x1b]2;", &mut forward, |_| {});
    let chunk = vec![b'x'; 16 * 1024];
    for _ in 0..640 {
        scanner.feed(&chunk, &mut forward, |_| {});
        assert!(scanner.held_capacity() <= MAX_SEQUENCE * 2);
    }
    scanner.feed(b"\x07b\x1b]0;ok\x07", &mut forward, |_| {});
    // The ESC went ahead; a CAN stands for the dropped rest, the next sequence passes.
    assert_eq!(forward, b"a\x1b\x18b\x1b]0;ok\x07");
    assert!(scanner.held_capacity() <= MAX_PAYLOAD);
}

#[test]
fn a_dropped_sequence_ended_by_esc_leaves_the_parser_in_its_escape_state() {
    let mut long = b"\x1b]2;".to_vec();
    long.extend(std::iter::repeat_n(b'x', MAX_SEQUENCE + 1));
    long.extend(b"\x1b\\z");
    assert_eq!(scan(&[&long]).1, b"\x1b\x18\x1b\\z");
}

#[test]
fn osc_52_may_carry_a_mebibyte_and_no_more() {
    let mut fits = b"\x1b]52;c;".to_vec();
    fits.extend(std::iter::repeat_n(b'A', MAX_CLIPBOARD_SEQUENCE - 8));
    fits.push(0x07);
    assert_eq!(scan(&[&fits]).1, fits);
    let mut long = b"\x1b]52;c;".to_vec();
    long.extend(std::iter::repeat_n(b'A', MAX_CLIPBOARD_SEQUENCE + 1));
    long.push(0x07);
    assert_eq!(scan(&[&long]).1, b"\x1b\x18");
    // Any other sequence of that length is dropped at its own limit.
    let mut title = b"\x1b]2;".to_vec();
    title.extend(std::iter::repeat_n(b'A', MAX_SEQUENCE));
    title.push(0x07);
    assert_eq!(scan(&[&title]).1, b"\x1b\x18");
}

#[test]
fn an_osc_after_a_control_in_the_escape_state_is_still_seen() {
    // The parser executes the BEL and stays in its escape state: `]` opens an OSC there.
    let mut long = b"\x1b\x07]2;".to_vec();
    long.extend(std::iter::repeat_n(b'x', MAX_SEQUENCE + 1));
    long.push(0x07);
    assert_eq!(scan(&[&long]).1, b"\x1b\x07\x18");
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

#[test]
fn a_title_loses_bidi_and_control_characters_and_is_cut_to_its_limit() {
    use super::{MAX_TITLE, clean_title};
    assert_eq!(
        clean_title("a\u{202e}b\u{2066}c\u{2069}d\u{202a}e\u{2068}f\tg\n"),
        "abcdefg"
    );
    // Other right-to-left text stays: only the controls go.
    assert_eq!(
        clean_title("\u{5e9}\u{5dc}\u{5d5}\u{5dd}"),
        "\u{5e9}\u{5dc}\u{5d5}\u{5dd}"
    );
    let long = "\u{e4}".repeat(MAX_TITLE * 4);
    assert_eq!(clean_title(&long).chars().count(), MAX_TITLE);
}
