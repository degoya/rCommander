use super::*;

#[test]
fn a_pattern_matches_and_says_where_the_match_ends() {
    let pattern = Pattern::new("(?i)password.*:").expect("compiles");
    assert_eq!(pattern.as_str(), "(?i)password.*:");
    assert_eq!(pattern.match_end("[sudo] PASSWORD for me: "), Some(23));
    assert_eq!(pattern.match_end("login: "), None);
}

#[test]
fn bad_patterns_are_refused_with_a_reason() {
    assert_eq!(Pattern::new("").err(), Some(PatternError::Empty));
    let long = "a".repeat(MAX_PATTERN_CHARS + 1);
    assert_eq!(
        Pattern::new(&long).err(),
        Some(PatternError::TooLong {
            max: MAX_PATTERN_CHARS
        })
    );
    assert!(matches!(
        Pattern::new("(unclosed"),
        Err(PatternError::Syntax { .. })
    ));
    // Look-around and back-references do not exist in a linear-time engine.
    assert!(Pattern::new(r"(a)\1").is_err());
}

#[test]
fn a_pattern_too_large_once_compiled_is_refused() {
    // Short to write, huge to compile: nested counted repetition.
    let error = Pattern::new(r"(\w{100}){100}").expect_err("over the size limit");
    assert!(matches!(error, PatternError::Syntax { .. }), "{error:?}");
}

#[test]
fn a_pattern_reads_and_writes_as_its_source() {
    let pattern = Pattern::new(r"#\s*$").expect("compiles");
    let json = serde_json::to_string(&pattern).expect("serializes");
    assert_eq!(json, r##""#\\s*$""##);
    let back: Pattern = serde_json::from_str(&json).expect("deserializes");
    assert_eq!(back, pattern);
    assert!(serde_json::from_str::<Pattern>(r#""(""#).is_err());
}
