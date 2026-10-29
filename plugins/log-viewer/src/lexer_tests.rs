use super::*;
use crate::types::TokenClass::{
    Comment, Function, Invalid, Key, Literal, Number, Punctuation, TypeName,
};

const STRING: TokenClass = TokenClass::String;

/// The tokens of each line of `text`, as the text they cover, lexed in order from the start.
fn lines(text: &str, colour: LevelColour) -> Vec<Vec<(&str, TokenClass)>> {
    let mut state = LineState::default();
    text.split('\n')
        .map(|line| {
            let line = line.strip_suffix('\r').unwrap_or(line);
            let mut tokens = Vec::new();
            state = lex_line(line.as_bytes(), state, colour, Some(&mut tokens));
            tokens
                .iter()
                .map(|token| {
                    let start = token.start as usize;
                    let end = start + token.len as usize;
                    (
                        line.get(start..end).expect("on character boundaries"),
                        token.class,
                    )
                })
                .collect()
        })
        .collect()
}

fn one(line: &str) -> Vec<(&str, TokenClass)> {
    lines(line, LevelColour::Word).remove(0)
}

/// The classes of each line of `text` with `level_colour = line`.
fn whole_lines(text: &str) -> Vec<Vec<TokenClass>> {
    lines(text, LevelColour::Line)
        .into_iter()
        .map(|tokens| tokens.into_iter().map(|(_, class)| class).collect())
        .collect()
}

#[test]
fn every_level_in_upper_lower_and_capitalised_case() {
    for (word, class) in [
        ("error", Invalid),
        ("fatal", Invalid),
        ("critical", Invalid),
        ("severe", Invalid),
        ("panic", Invalid),
        ("err", Invalid),
        ("crit", Invalid),
        ("alert", Invalid),
        ("emerg", Invalid),
        ("warn", Function),
        ("warning", Function),
        ("info", TypeName),
        ("notice", TypeName),
        ("debug", Comment),
        ("trace", Comment),
        ("verbose", Comment),
    ] {
        let upper = word.to_ascii_uppercase();
        let capital = format!("{}{}", &upper[..1], &word[1..]);
        for form in [word.to_owned(), upper, capital] {
            let line = format!("x {form}: done");
            assert_eq!(one(&line), vec![(form.as_str(), class)], "{form}");
        }
    }
    // Mixed case is no level, nor a level word inside a longer word or name.
    for line in [
        "x eRRor done",
        "ERRORS: 0",
        "information only",
        "Warnings",
        "infos",
        "error.log rotated",
        "on-error handler",
        "my_error",
        "ERROR2",
    ] {
        assert_eq!(one(line), vec![], "{line}");
    }
}

#[test]
fn bracketed_keyed_and_logcat_levels() {
    assert_eq!(
        one("[ERROR] x"),
        vec![("[", Punctuation), ("ERROR", Invalid), ("]", Punctuation)]
    );
    assert_eq!(
        one("level=error msg=boom"),
        vec![("level", Key), ("error", Invalid), ("msg", Key)]
    );
    assert_eq!(one("E/ActivityManager: died"), vec![("E", Invalid)]);
    assert_eq!(one("W/Tag( 123): slow"), vec![("W", Function)]);
    assert_eq!(one("I/O error"), vec![("error", Invalid)]);
    assert_eq!(
        one("10-20 15:20:01.123  1234  1234 E Tag: boom"),
        vec![("10-20 15:20:01.123", Number), ("E", Invalid)]
    );
    assert_eq!(
        one("D1020 15:20:01.123456  1234 main.cc:42] x"),
        vec![
            ("D", Comment),
            ("1020 15:20:01.123456", Number),
            ("]", Punctuation)
        ]
    );
}

#[test]
fn every_timestamp_form_is_a_number() {
    for stamp in [
        "2026-10-20",
        "2026-10-20T15:20:01Z",
        "2026-10-20T15:20:01.123+02:00",
        "2026-10-20T15:20:01.123456789-0700",
        "2026-10-20 15:20:01,123",
        "2026-10-20 15:20",
        "2026/10/20 15:20:01",
        "15:20:01",
        "15:20:01.123",
        "Oct 20 15:20:01",
        "Oct  1 15:20:01",
        "Oct 20 15:20:01.123456 2026",
        "20/Oct/2026:15:20:01 +0200",
    ] {
        assert_eq!(
            one(&format!("x {stamp} y")),
            vec![(stamp, Number)],
            "{stamp}"
        );
    }
    // A `/` alone is no path.
    assert_eq!(one("1792502401.5 GET /"), vec![("1792502401.5", Number)]);
    assert_eq!(
        one("[1792502401] up"),
        vec![
            ("[", Punctuation),
            ("1792502401", Number),
            ("]", Punctuation)
        ]
    );
    // CBS.log: the comma after the seconds starts no fraction.
    assert_eq!(
        one("2026-10-20 15:20:01, Info                  CBS    Loaded"),
        vec![("2026-10-20 15:20:01", Number), ("Info", TypeName)]
    );
    // Unix seconds only at a line's start; digits joined to a word are no timestamp.
    for line in [
        "x 1792502401 y",
        "v2026-10-20",
        "15:20:01x",
        "Octopus 20 15:20:01x",
    ] {
        assert!(
            one(line).iter().all(|(_, class)| *class != Number),
            "{line}"
        );
    }
}

#[test]
fn addresses_urls_and_paths_are_literals() {
    for literal in [
        "192.168.1.10",
        "10.0.0.1:8080",
        "2001:db8::1",
        "fe80::1",
        "::1",
        "https://example.com/a?b=c&d=e",
        "ftp://files.example.com/x",
        r"C:\Windows\Logs\CBS\CBS.log",
        "C:/temp/x.txt",
        r"\\server\share\x",
        "/var/log/nginx/access.log",
        "./run.sh",
        "../x/y",
        "~/logs",
    ] {
        assert_eq!(
            one(&format!("at {literal} done")),
            vec![(literal, Literal)],
            "{literal}"
        );
    }
    // The sentence's punctuation after it is not part of it.
    assert_eq!(
        one("see (/etc/hosts), or http://x.org."),
        vec![("/etc/hosts", Literal), ("http://x.org", Literal)]
    );
    for line in [
        "1.2.3.4.5",
        "256.1.1.1",
        "and/or 1/2",
        "HTTP/1.1",
        "std::vec::Vec",
        "00:1a:2b:3c:4d:5e",
    ] {
        assert!(
            one(line).iter().all(|(_, class)| *class != Literal),
            "{line}"
        );
    }
}

#[test]
fn a_json_line_colours_keys_strings_levels_and_timestamps() {
    assert_eq!(
        one(r#"{"ts":"2026-10-20T15:20:01Z","level":"error","msg":"disk \"full\"","n":3}"#),
        vec![
            (r#""ts""#, Key),
            (r#""2026-10-20T15:20:01Z""#, Number),
            (r#""level""#, Key),
            (r#""error""#, Invalid),
            (r#""msg""#, Key),
            (r#""disk \"full\"""#, STRING),
            (r#""n""#, Key),
        ]
    );
    assert_eq!(
        one("file 'x.txt' can't be read, users' 'y'"),
        vec![("'x.txt'", STRING), ("'y'", STRING)]
    );
    // A quote left open colours nothing after it.
    assert_eq!(one(r#"x "open ERROR"#), vec![("ERROR", Invalid)]);
}

const PYTHON: &str = "2026-10-20 15:20:01,123 ERROR app: failed\n\
Traceback (most recent call last):\n  \
File \"/srv/app.py\", line 3, in <module>\n    \
main()\n\
ZeroDivisionError: division by zero\n\
2026-10-20 15:20:02,000 INFO app: retry";

const JAVA: &str = "2026-10-20 15:20:01.123 WARN [main] c.e.App - slow\n\
java.lang.IllegalStateException: boom\n\
\tat com.example.App.run(App.java:10)\n\
\tat com.example.App.main(App.java:4)\n\
Caused by: java.io.IOException: x\n\
\t... 2 more\n\
2026-10-20 15:20:02.000 DEBUG [main] c.e.App - next";

#[test]
fn stack_traces_are_type_names_word_by_word() {
    let python = lines(PYTHON, LevelColour::Word);
    assert_eq!(
        python[1],
        vec![("Traceback (most recent call last):", TypeName)]
    );
    assert_eq!(
        python[2],
        vec![(r#"File "/srv/app.py", line 3, in <module>"#, TypeName)]
    );
    assert_eq!(python[3], vec![]);
    assert_eq!(python[4], vec![]);
    let java = lines(JAVA, LevelColour::Word);
    assert_eq!(java[0][1], ("WARN", Function));
    assert_eq!(java[1], vec![]);
    assert_eq!(
        java[2],
        vec![("at com.example.App.run(App.java:10)", TypeName)]
    );
    assert_eq!(
        java[4],
        vec![("Caused by: java.io.IOException: x", TypeName)]
    );
    assert_eq!(java[5], vec![("... 2 more", TypeName)]);
    // `at` in a sentence is no frame.
    assert_eq!(one("  at least once"), vec![]);
}

#[test]
fn level_colour_line_colours_the_record_and_its_stack_trace() {
    assert_eq!(
        whole_lines(PYTHON),
        vec![
            vec![Invalid],
            vec![Invalid],
            vec![Invalid],
            vec![Invalid],
            vec![Invalid],
            vec![Number, TypeName],
        ]
    );
    assert_eq!(
        whole_lines(JAVA),
        vec![
            vec![Function],
            vec![Function],
            vec![Function],
            vec![Function],
            vec![Function],
            vec![Function],
            vec![Number, Comment, Punctuation, Punctuation],
        ]
    );
    // A Python traceback is an error even without a level before it; an empty line ends it.
    assert_eq!(
        whole_lines("Traceback (most recent call last):\n  File \"x.py\", line 1\n\nnext"),
        vec![vec![Invalid], vec![Invalid], vec![], vec![]]
    );
    // An information line keeps its parts; the record of an error does not reach past it.
    assert_eq!(
        whole_lines("ERROR a\nINFO b\n  indented"),
        vec![vec![Invalid], vec![TypeName], vec![]]
    );
    let line = lines("x ERROR at 15:20:01", LevelColour::Line).remove(0);
    assert_eq!(line, vec![("x ERROR at 15:20:01", Invalid)]);
}

#[test]
fn the_state_alone_is_the_state_the_tokens_reach() {
    let text = format!("{PYTHON}\n{JAVA}");
    let mut with = LineState::default();
    let mut without = LineState::default();
    for line in text.split('\n') {
        let mut tokens = Vec::new();
        with = lex_line(line.as_bytes(), with, LevelColour::Line, Some(&mut tokens));
        without = lex_line(line.as_bytes(), without, LevelColour::Line, None);
        assert_eq!(with, without, "{line}");
    }
}

#[test]
fn a_line_gets_at_most_4096_tokens_and_its_level_still_counts() {
    let line = format!("{} ERROR", "[x] ".repeat(3000));
    let tokens = one(&line);
    assert_eq!(tokens.len(), MAX_TOKENS_PER_LINE);
    assert!(tokens.iter().all(|(_, class)| *class == Punctuation));
    // The level past the 4096th token still makes the whole line an error.
    assert_eq!(whole_lines(&line), vec![vec![Invalid]]);
}

#[test]
fn text_beyond_ascii_is_part_of_a_word() {
    assert_eq!(
        one("Fehler: Datei \u{e4}rger ERROR \u{fffd}error"),
        vec![("ERROR", Invalid)]
    );
    assert_eq!(
        one("Datei /tmp/\u{e4}rger.log fehlt: ERROR"),
        vec![("/tmp/\u{e4}rger.log", Literal), ("ERROR", Invalid)]
    );
}
