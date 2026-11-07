use super::*;
use crate::types::Token;

/// The inline tokens of `text`, each as its text.
fn marked(text: &str) -> Vec<(&str, TokenClass)> {
    let mut tokens: Vec<Token> = Vec::new();
    {
        let mut cur = Cursor::new(text.as_bytes(), &mut tokens);
        inline(&mut cur, text.len());
    }
    tokens
        .iter()
        .map(|token| {
            let start = token.start as usize;
            (&text[start..start + token.len as usize], token.class)
        })
        .collect()
}

#[test]
fn markup_needs_a_start_and_an_end_in_their_places() {
    // A blank after the start, a letter before it, a blank before the end: no markup.
    assert!(marked("2 * 3 * 4").is_empty());
    assert!(marked("a*b* c").is_empty());
    assert!(marked("*a * b").is_empty());
    assert!(marked("``open literal").is_empty());
    // After an opening bracket or a quote it may start.
    assert_eq!(marked("(*a*)"), vec![("*a*", TokenClass::Literal)]);
    assert_eq!(marked("\"**b**\""), vec![("**b**", TokenClass::Keyword)]);
}

#[test]
fn roles_references_and_addresses() {
    assert_eq!(
        marked(":py:func:`run` and `x`:math:"),
        vec![
            (":py:func:", TokenClass::Function),
            ("`run`", TokenClass::String),
            ("`x`", TokenClass::String),
            (":math:", TokenClass::Function)
        ]
    );
    assert_eq!(
        marked("anonymous__ and `two words`__ and rc-plugin.name_"),
        vec![
            ("anonymous__", TokenClass::TypeName),
            ("`two words`__", TokenClass::TypeName),
            ("rc-plugin.name_", TokenClass::TypeName)
        ]
    );
    assert_eq!(
        marked("see https://example.org/a_b."),
        vec![("https://example.org/a_b", TokenClass::TypeName)]
    );
    assert!(marked("snake_case and _private").is_empty());
    assert_eq!(
        marked("|logo|_ [#note]_ [CIT2002]_"),
        vec![
            ("|logo|_", TokenClass::Key),
            ("[#note]_", TokenClass::Key),
            ("[CIT2002]_", TokenClass::Key)
        ]
    );
}

#[test]
fn an_end_string_is_looked_for_a_bounded_distance() {
    let far = format!("*a{}*", "x".repeat(MAX_INLINE + 10));
    assert!(marked(&far).is_empty());
    let near = format!("*a{}*", "x".repeat(100));
    assert_eq!(marked(&near).len(), 1);
}
