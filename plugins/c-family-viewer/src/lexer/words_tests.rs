use super::*;

/// The number at the start of `text`.
fn number(lang: Lang, text: &str) -> &str {
    &text[..number_end(lang, text.as_bytes(), 0, false)]
}

#[test]
fn numbers_take_their_prefixes_suffixes_and_separators() {
    for (text, wanted) in [
        ("42;", "42"),
        ("0xFF)", "0xFF"),
        ("0b1010_0101,", "0b1010_0101"),
        ("0o17 ", "0o17"),
        ("1_000_000", "1_000_000"),
        ("1.5f;", "1.5f"),
        (".5 ", ".5"),
        ("6.02e+23;", "6.02e+23"),
        ("1E-9)", "1E-9"),
        ("0x1p-3;", "0x1p-3"),
        ("0x1e+5", "0x1e"),
        ("10UL;", "10UL"),
        ("100L)", "100L"),
        ("2.0m;", "2.0m"),
        ("255u8,", "255u8"),
        ("1e3_f64;", "1e3_f64"),
        ("1..10", "1"),
        ("1.max(2)", "1"),
        ("1.2.3", "1.2"),
    ] {
        assert_eq!(number(Lang::Rust, text), wanted, "{text}");
    }
}

#[test]
fn a_quote_separates_digits_in_c_and_cpp_only() {
    assert_eq!(number(Lang::Cpp, "1'000'000;"), "1'000'000");
    assert_eq!(number(Lang::C, "0x7FFF'FFFF)"), "0x7FFF'FFFF");
    assert_eq!(number(Lang::Cpp, "1' "), "1");
    assert_eq!(number(Lang::Rust, "1'a"), "1");
}

#[test]
fn a_member_number_takes_no_fraction() {
    assert_eq!(number_end(Lang::Rust, b"0.1", 0, true), 1);
}

#[test]
fn operators_are_the_longest_that_fit() {
    for (text, width) in [
        (">>>=", 4),
        ("<=> x", 3),
        ("..=5", 3),
        ("->x", 2),
        ("=> {", 2),
        ("?.let", 2),
        ("?:", 2),
        ("!!", 2),
        (":=", 2),
        ("<-ch", 2),
        ("===", 3),
        ("+", 1),
        ("#", 1),
        ("##", 2),
    ] {
        assert_eq!(operator_width(text.as_bytes()), Some(width), "{text}");
    }
    assert_eq!(operator_width(b"("), None);
    assert_eq!(operator_width(b";"), None);
}

#[test]
fn names_take_a_dollar_where_the_language_does() {
    assert_eq!(name_end(Lang::Java, b"$tmp1 =", 0), 5);
    assert_eq!(name_end(Lang::Kotlin, b"a$b", 0), 1);
    assert_eq!(name_end(Lang::CSharp, b"x$", 0), 1);
    assert!(!is_name_start(Lang::C, b'1'));
    assert!(is_name_start(Lang::Rust, 0xC3));
}

#[test]
fn runs_are_counted_up_to_255() {
    assert_eq!(run_of(b"\"\"\"x", 0, b'"'), 3);
    assert_eq!(run_of(b"x", 0, b'"'), 0);
    assert_eq!(run_of(b"ab", 5, b'a'), 0);
    assert_eq!(run_of(&[b'#'; 300], 0, b'#'), 255);
}
