use super::*;
use crate::types::Token;

fn heredoc(text: &str, who: Interp) -> Option<(usize, Heredoc)> {
    let mut tokens: Vec<Token> = Vec::new();
    let cur = Cursor::new(text.as_bytes(), &mut tokens);
    heredoc_at(&cur, 0, who)
}

fn variable(text: &str) -> Option<usize> {
    let mut tokens: Vec<Token> = Vec::new();
    let cur = Cursor::new(text.as_bytes(), &mut tokens);
    perl_variable(&cur, 0)
}

#[test]
fn brackets_close_with_their_partner() {
    assert_eq!(closing(b'('), b')');
    assert_eq!(closing(b'['), b']');
    assert_eq!(closing(b'{'), b'}');
    assert_eq!(closing(b'<'), b'>');
    assert_eq!(closing(b'|'), b'|');
    assert_eq!(quote(b'{', true).close, b'}');
}

#[test]
fn heredoc_openings_by_language() {
    let (end, found) = heredoc("<<~EOS rest", Interp::Ruby).expect("a heredoc");
    assert_eq!(
        (end, found.tag.bytes(), found.indented, found.interp),
        (6, &b"EOS"[..], true, true)
    );
    let (end, found) = heredoc("<<-'SQL'", Interp::Ruby).expect("a heredoc");
    assert_eq!((end, found.indented, found.interp), (8, true, false));
    // `<<-` is Ruby's; in Perl a bare tag of any case is one.
    assert!(heredoc("<<-EOS", Interp::Perl).is_none());
    assert!(heredoc("<<eos", Interp::Ruby).is_none());
    assert_eq!(heredoc("<<eos;", Interp::Perl).map(|(end, _)| end), Some(5));
    assert!(heredoc("<< EOS", Interp::Ruby).is_none());
    assert!(heredoc("<\"x\"", Interp::Perl).is_none());
    assert!(heredoc("<<\"open", Interp::Perl).is_none());
    // A tag longer than MAX_TAG opens nothing.
    let long = format!("<<{}", "A".repeat(super::super::MAX_TAG + 1));
    assert!(heredoc(&long, Interp::Ruby).is_none());
}

#[test]
fn perl_variables_inside_strings() {
    assert_eq!(variable("$name rest"), Some(5));
    assert_eq!(variable("$My::Pkg::x."), Some(11));
    assert_eq!(variable("${name}x"), Some(7));
    assert_eq!(variable("@list,"), Some(5));
    assert_eq!(variable("$$ref"), Some(5));
    assert_eq!(variable("$1"), Some(2));
    assert_eq!(variable("$/"), None);
    assert_eq!(variable("$"), None);
    assert_eq!(variable("@ "), None);
    assert_eq!(variable("${never closed"), None);
}
