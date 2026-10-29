use super::CaseMode;

#[test]
fn every_mode_changes_the_case_as_named() {
    let cases = [
        (CaseMode::Unchanged, "hELLO wORLD-x_y", "hELLO wORLD-x_y"),
        (CaseMode::Lower, "hELLO wORLD-x_y", "hello world-x_y"),
        (CaseMode::Upper, "hELLO wORLD-x_y", "HELLO WORLD-X_Y"),
        (CaseMode::FirstUpper, "hELLO wORLD-x_y", "Hello world-x_y"),
        (CaseMode::WordsUpper, "hELLO wORLD-x_y", "Hello World-X_Y"),
        (CaseMode::WordsUpper, "track 01b", "Track 01b"),
        (CaseMode::FirstUpper, "", ""),
        (CaseMode::WordsUpper, "", ""),
        // Not only ASCII: a-umlaut, sharp s.
        (CaseMode::Upper, "\u{e4}rger", "\u{c4}RGER"),
        (CaseMode::Upper, "stra\u{df}e", "STRASSE"),
        (
            CaseMode::WordsUpper,
            "\u{e4}pfel \u{f6}l",
            "\u{c4}pfel \u{d6}l",
        ),
    ];
    for (mode, text, expected) in cases {
        assert_eq!(mode.apply(text), expected, "{mode:?} {text}");
    }
}

#[test]
fn modes_read_and_write_as_snake_case() {
    let mode: CaseMode = serde_json::from_str("\"words_upper\"").expect("parses");
    assert_eq!(mode, CaseMode::WordsUpper);
    assert_eq!(
        serde_json::to_string(&CaseMode::FirstUpper).expect("serialises"),
        "\"first_upper\""
    );
}

#[test]
fn appending_continues_the_words_of_what_is_there() {
    let cases = [
        (CaseMode::WordsUpper, "", "hELLO", "Hello"),
        (CaseMode::WordsUpper, "x ", "hELLO", "x Hello"),
        (CaseMode::WordsUpper, "xy", "hELLO", "xyhello"),
        (CaseMode::FirstUpper, "", "hELLO", "Hello"),
        (CaseMode::FirstUpper, "x", "hELLO", "xhello"),
        (CaseMode::Upper, "ab", "cd", "abCD"),
    ];
    for (mode, before, text, expected) in cases {
        let mut out = before.to_owned();
        mode.append(&mut out, text);
        assert_eq!(out, expected, "{mode:?} {before:?} + {text:?}");
    }
}
