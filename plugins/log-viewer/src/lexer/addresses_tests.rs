use super::*;

/// What `matcher` takes of `text` from `at`.
fn taken(text: &str, at: usize, matcher: fn(&[u8], usize) -> Option<usize>) -> Option<&str> {
    matcher(text.as_bytes(), at).map(|end| &text[at..end])
}

#[test]
fn ipv4_parts_and_port() {
    assert_eq!(taken("10.0.0.1:8080,", 0, ipv4), Some("10.0.0.1:8080"));
    // Six digits are no port: the address ends before the colon.
    assert_eq!(taken("10.0.0.1:123456", 0, ipv4), Some("10.0.0.1"));
    assert_eq!(taken("10.0.0.1.", 0, ipv4), Some("10.0.0.1"));
    for text in ["10.0.0", "1000.0.0.1", "10.0.0.1x", "v10.0.0.1"] {
        let at = usize::from(text.starts_with('v'));
        assert_eq!(taken(text, at, ipv4), None, "{text}");
    }
}

#[test]
fn ipv6_groups() {
    assert_eq!(
        taken("2001:0db8:85a3:0000:0000:8a2e:0370:7334 x", 0, ipv6),
        Some("2001:0db8:85a3:0000:0000:8a2e:0370:7334")
    );
    assert_eq!(taken("fe80::1: up", 0, ipv6), Some("fe80::1"));
    assert_eq!(taken("[::1]:8080", 1, ipv6), Some("::1"));
    for text in [
        "1::2::3",
        "12345::1",
        "1:2:3:4:5:6:7",
        "15:20:01",
        "::",
        "a:::b",
    ] {
        assert_eq!(taken(text, 0, ipv6), None, "{text}");
    }
}

#[test]
fn urls_and_paths_run_to_a_space_or_quote_without_the_sentence() {
    assert_eq!(
        taken("https://x.org/a(b)?c=1\" y", 0, url),
        Some("https://x.org/a(b)?c=1")
    );
    assert_eq!(taken("x://", 0, url), None);
    assert_eq!(taken("://x", 0, url), None);
    assert_eq!(
        taken("/var/log/x.log:12:", 0, path),
        Some("/var/log/x.log:12")
    );
    assert_eq!(taken(r"C:\x\y.cs]", 0, path), Some(r"C:\x\y.cs"));
    assert_eq!(taken("a/b", 1, path), None);
    assert_eq!(taken("//", 0, path), None);
    assert_eq!(taken("C:x", 0, path), None);
}
