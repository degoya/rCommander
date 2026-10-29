use super::*;
use crate::Viewer;
use crate::lexer::MAX_PIECE;
use crate::test_support::named;
use crate::types::TokenClass;

#[test]
fn a_minified_line_gets_at_most_4096_tokens() {
    let text = "a;".repeat(5000);
    let mut viewer = Viewer::open(named(text.as_bytes(), "app.min.js"))
        .expect("opens")
        .0;
    let tokens = viewer.tokens(0, 1).expect("tokens");
    assert_eq!(tokens[0].len(), MAX_TOKENS_PER_LINE);
    assert!(
        tokens[0]
            .iter()
            .all(|token| token.class == TokenClass::Punctuation)
    );
}

#[test]
fn a_long_line_is_coloured_in_pieces_up_to_64_kib() {
    let text = format!("x = \"{}\";\ndone();\n", "s".repeat(100 * 1024));
    let mut viewer = Viewer::open(named(text.as_bytes(), "a.js"))
        .expect("opens")
        .0;
    let tokens = viewer.tokens(0, 2).expect("tokens");
    let long = &tokens[0];
    assert!(long.iter().all(|token| token.len <= MAX_PIECE));
    assert!(
        long.windows(2)
            .all(|pair| pair[0].start + pair[0].len <= pair[1].start)
    );
    let end = long
        .last()
        .map_or(0, |token| (token.start + token.len) as usize);
    assert!(end <= MAX_LINE_BYTES, "{end}");
    assert!(end > MAX_LINE_BYTES - MAX_PIECE as usize, "{end}");
    // The line after it is read from its own checkpoint, in the state the long line left.
    assert_eq!(
        tokens[1].first(),
        Some(&Token {
            start: 0,
            len: 4,
            class: TokenClass::Function
        })
    );
}
