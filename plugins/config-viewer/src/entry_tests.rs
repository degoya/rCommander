use super::*;
use crate::lexer::lex_line;
use crate::types::Format;

fn read(line: &str, format: Format) -> (LineKind<'_>, (String, Option<String>)) {
    let mut tokens = Vec::new();
    let next = lex_line(line.as_bytes(), LineState::Clear, format, Some(&mut tokens));
    (
        line_kind(line, LineState::Clear, &tokens),
        entry(line, &tokens, next != LineState::Clear),
    )
}

fn header(line: &str, format: Format) -> LineKind<'_> {
    read(line, format).0
}

fn pair(label: &str, value: Option<&str>) -> (String, Option<String>) {
    (label.to_owned(), value.map(str::to_owned))
}

#[test]
fn headers_and_lines_that_are_neither() {
    assert_eq!(
        header("[General]", Format::Ini),
        LineKind::Header {
            array: false,
            name: "General"
        }
    );
    assert_eq!(
        header("[[bin]]", Format::Toml),
        LineKind::Header {
            array: true,
            name: "bin"
        }
    );
    assert_eq!(
        header("[ a.b ] # c", Format::Toml),
        LineKind::Header {
            array: false,
            name: "a.b"
        }
    );
    assert_eq!(header("; c", Format::Ini), LineKind::Other);
    assert_eq!(header("", Format::Ini), LineKind::Other);
    assert_eq!(
        header("Windows Registry Editor Version 5.00", Format::Registry),
        LineKind::Other
    );
    // A line inside a value of the line before is no entry.
    let mut tokens = Vec::new();
    lex_line(b"k = 1", LineState::Clear, Format::Ini, Some(&mut tokens));
    assert_eq!(
        line_kind("k = 1", LineState::Continued, &tokens),
        LineKind::Other
    );
}

#[test]
fn entries_of_every_format() {
    for (line, format, expected) in [
        ("name = value ; c", Format::Ini, pair("name", Some("value"))),
        ("bare", Format::Ini, pair("bare", None)),
        (
            "\"Count\"=dword:0000001f",
            Format::Registry,
            pair("Count", Some("dword:0000001f")),
        ),
        ("@=\"x\"", Format::Registry, pair("@", Some("\"x\""))),
        ("export KEY=v", Format::Env, pair("KEY", Some("v"))),
        ("a.\"b\" = 1 # c", Format::Toml, pair("a.\"b\"", Some("1"))),
        (
            "'k' = { x = 1 }",
            Format::Toml,
            pair("k", Some("{ x = 1 }")),
        ),
        ("key value", Format::Properties, pair("key", Some("value"))),
        (
            "list = a,\\",
            Format::Properties,
            pair("list", Some("a,\\\u{2026}")),
        ),
        (
            "s = \"\"\"open",
            Format::Toml,
            pair("s", Some("\"\"\"open\u{2026}")),
        ),
        ("*.txt text", Format::Lines, pair("*.txt", Some("text"))),
    ] {
        let (kind, got) = read(line, format);
        assert_eq!(kind, LineKind::Entry, "{line}");
        assert_eq!(got, expected, "{line}");
    }
}
