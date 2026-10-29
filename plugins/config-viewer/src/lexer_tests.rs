use super::*;

#[test]
fn bare_values_are_numbers_literals_or_strings() {
    for word in ["0", "-12", "+3.25", "0x1F", "8080"] {
        assert_eq!(scalar_class(word.as_bytes()), TokenClass::Number, "{word}");
    }
    for word in ["true", "False", "ON", "off", "yes", "No"] {
        assert_eq!(scalar_class(word.as_bytes()), TokenClass::Literal, "{word}");
    }
    for word in ["", "1.2.3", "0x", "abc", "12px", "-", "truely"] {
        assert_eq!(scalar_class(word.as_bytes()), TokenClass::String, "{word}");
    }
}

#[test]
fn toml_levels_remember_their_kind() {
    let mut state = TomlState::default();
    assert!(!state.in_table());
    state.open(false);
    state.open(true);
    assert!(state.in_table());
    assert_eq!(state.end(), LineState::Toml(state));
    assert!(state.close(true));
    assert!(!state.in_table());
    assert!(!state.close(true), "an array closed with a brace");
    assert!(!state.close(false), "nothing open");
    assert_eq!(state.end(), LineState::Clear);
    // Deeper than the kept levels: counted, closed by either bracket.
    for _ in 0..40 {
        state.open(true);
    }
    assert_eq!(state.depth, 40);
    assert!(state.close(false));
    assert_eq!(state.depth, 39);
}

#[test]
fn helpers_keep_to_the_line() {
    let text = b"  key = \"a\\\"b\" ; note";
    assert_eq!(skip_space(text, 0), 2);
    assert_eq!(trim_end(text, 0, 6), 5);
    assert_eq!(closing(text, 9, b'"', true), Some(14));
    assert_eq!(closing(text, 9, b'"', false), Some(12));
    assert_eq!(closing(b"\"open", 1, b'"', true), None);
    assert_eq!(comment_start(text, 0, b";"), Some(15));
    assert_eq!(comment_start(b"a;b", 0, b";"), None);
    assert_eq!(char_end("a\u{e4}".as_bytes(), 1), 3);
    assert_eq!(char_end("\u{1f600}".as_bytes(), 0), 4);
}

#[test]
fn no_line_gets_more_than_the_limit_of_tokens() {
    let line = "k=".to_owned() + &"1,".repeat(5000);
    let mut tokens = Vec::new();
    lex_line(
        line.as_bytes(),
        LineState::Clear,
        Format::Ini,
        Some(&mut tokens),
    );
    assert_eq!(tokens.len(), MAX_TOKENS_PER_LINE);
    // Without a list the lexer only follows the state.
    assert_eq!(
        lex_line(b"a=\"x", LineState::Clear, Format::Env, None),
        LineState::Quoted(b'"')
    );
}
