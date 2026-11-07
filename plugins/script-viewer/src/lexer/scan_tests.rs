use super::*;
use crate::types::Token;

/// The tokens of `text` read as numbers and symbols alone, each as its text.
fn scanned(text: &str) -> Vec<(&str, TokenClass)> {
    let mut tokens: Vec<Token> = Vec::new();
    {
        let mut cur = Cursor::new(text.as_bytes(), &mut tokens);
        while let Some(byte) = cur.byte(cur.pos) {
            if byte.is_ascii_digit() {
                cur.number();
            } else if byte == b' ' {
                cur.pos += 1;
            } else {
                cur.symbol();
            }
        }
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
fn numbers_in_every_form() {
    for number in [
        "0", "42", "1_000", "0.19", "1.5e3", "6.02E+23", "0x1F", "0xFFp4", "0b1010", "0o17", "3r",
        "2i",
    ] {
        assert_eq!(
            scanned(number),
            vec![(number, TokenClass::Number)],
            "{number}"
        );
    }
    // `1..10` is a range, `1e+` without digits no exponent.
    assert_eq!(
        scanned("1..10"),
        vec![
            ("1", TokenClass::Number),
            ("..", TokenClass::Operator),
            ("10", TokenClass::Number)
        ]
    );
    assert_eq!(scanned("1e+")[0], ("1e", TokenClass::Number));
}

#[test]
fn brackets_separators_and_operator_runs() {
    assert_eq!(
        scanned("(a) [b] {c}, ; . .. ||= => !~"),
        vec![
            ("(", TokenClass::Punctuation),
            (")", TokenClass::Punctuation),
            ("[", TokenClass::Punctuation),
            ("]", TokenClass::Punctuation),
            ("{", TokenClass::Punctuation),
            ("}", TokenClass::Punctuation),
            (",", TokenClass::Punctuation),
            (";", TokenClass::Punctuation),
            (".", TokenClass::Punctuation),
            ("..", TokenClass::Operator),
            ("||=", TokenClass::Operator),
            ("=>", TokenClass::Operator),
            ("!~", TokenClass::Operator)
        ]
    );
}

#[test]
fn a_closing_bracket_is_a_value_an_operator_is_none() {
    let mut tokens: Vec<Token> = Vec::new();
    let mut cur = Cursor::new(b") +", &mut tokens);
    cur.symbol();
    assert!(cur.value);
    cur.pos += 1;
    cur.symbol();
    assert!(!cur.value);
}

#[test]
fn an_inner_cursor_stops_at_its_end_and_reports_no_strings() {
    #[derive(Default)]
    struct Count(u32);
    impl super::super::Emit for Count {
        fn opened(&mut self, _col: usize, _width: u8) {
            self.0 += 1;
        }
    }
    let mut count = Count::default();
    let mut cur = Cursor::new(b"ab}cd", &mut count);
    let inner = cur.inner(0, 2);
    assert_eq!(inner.byte(1), Some(b'b'));
    assert_eq!(inner.byte(2), None);
    assert_eq!(inner.name_end(0), 2);
    let mut inner = inner;
    inner.opened(0, 1);
    cur.opened(0, 1);
    assert_eq!(count.0, 1);
}
