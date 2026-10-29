use std::sync::Mutex;

use super::{MAX_URL, open, opens, shown};

/// What `launch` was handed, by every test of the crate.
pub(super) static LAUNCHED: Mutex<Vec<String>> = Mutex::new(Vec::new());

fn launched(url: &str) -> bool {
    LAUNCHED
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .iter()
        .any(|seen| seen == url)
}

#[test]
fn only_http_https_and_mailto_open() {
    for url in [
        "http://example.com",
        "https://example.com/a?b=c#d",
        "HTTPS://EXAMPLE.COM",
        "mailto:someone@example.com",
    ] {
        assert!(opens(url), "{url} opens");
    }
    for url in [
        "javascript:alert(1)",
        "JavaScript:alert(1)",
        "file:///C:/Windows/System32/calc.exe",
        "file://server/share/x.exe",
        "vbscript:msgbox",
        "data:text/html,<script>alert(1)</script>",
        "ms-settings:privacy",
        "rcommander:open",
        "README.md",
        "./other.md#part",
        "C:\\Windows\\notepad.exe",
        "\\\\server\\share",
        "https:",
        "https:/x",
        "mailto:",
        "https://example.com/a b",
        "https://example.com/\n",
        "",
    ] {
        assert!(!opens(url), "{url:?} never opens");
    }
    let long = format!("https://example.com/{}", "a".repeat(MAX_URL));
    assert!(!opens(&long), "an overlong link never opens");
}

#[tokio::test]
async fn a_refused_link_reaches_no_handler_and_an_allowed_one_does() {
    assert!(!open("javascript:alert(1)".to_owned()).await);
    assert!(!open("file:///etc/passwd".to_owned()).await);
    assert!(!launched("javascript:alert(1)"));
    assert!(!launched("file:///etc/passwd"));
    assert!(open("https://example.com/markdown-preview".to_owned()).await);
    assert!(launched("https://example.com/markdown-preview"));
}

#[test]
fn a_shown_link_is_cut_and_has_no_control_characters() {
    assert_eq!(shown("https://a.example"), "https://a.example");
    assert_eq!(shown("a\u{7}b"), "a?b");
    let long = "x".repeat(200);
    let cut = shown(&long);
    assert_eq!(cut.chars().count(), 121);
    assert!(cut.ends_with('\u{2026}'));
}
