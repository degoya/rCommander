use super::{CounterSpec, CounterWidth, DatePart, Range, Token, parse};
use crate::case::CaseMode;
use crate::error::{PatternError, PatternErrorKind};

fn tokens(mask: &str) -> Vec<Token> {
    parse(mask)
        .unwrap_or_else(|error| panic!("{mask}: {error}"))
        .tokens()
        .to_vec()
}

#[test]
fn every_placeholder_parses_to_its_tokens() {
    let counter = |start, step, width| vec![Token::Counter(CounterSpec { start, step, width })];
    let digits = |width| Some(CounterWidth::Digits(width));
    let dates = |parts: &[DatePart]| parts.iter().map(|part| Token::Date(*part)).collect();
    let cases: Vec<(&str, Vec<Token>)> = vec![
        ("[N]", vec![Token::Name(Range::All)]),
        ("[N1]", vec![Token::Name(Range::At(1))]),
        ("[N-3]", vec![Token::Name(Range::At(-3))]),
        ("[N2-5]", vec![Token::Name(Range::Span(2, Some(5)))]),
        ("[N2,5]", vec![Token::Name(Range::Count(2, 5))]),
        ("[N2-]", vec![Token::Name(Range::Span(2, None))]),
        ("[N-8,5]", vec![Token::Name(Range::Count(-8, 5))]),
        // After a negative start the end counts from the end too.
        ("[N-8-5]", vec![Token::Name(Range::Span(-8, Some(-5)))]),
        ("[N-8--5]", vec![Token::Name(Range::Span(-8, Some(-5)))]),
        ("[N2--5]", vec![Token::Name(Range::Span(2, Some(-5)))]),
        ("[N-5-]", vec![Token::Name(Range::Span(-5, None))]),
        ("[E]", vec![Token::Extension(Range::All)]),
        ("[E1-2]", vec![Token::Extension(Range::Span(1, Some(2)))]),
        ("[A]", vec![Token::FileName(Range::All)]),
        ("[A-5-]", vec![Token::FileName(Range::Span(-5, None))]),
        ("[P]", vec![Token::Parent(Range::All)]),
        ("[P2-5]", vec![Token::Parent(Range::Span(2, Some(5)))]),
        ("[G]", vec![Token::Grandparent(Range::All)]),
        ("[G-8,5]", vec![Token::Grandparent(Range::Count(-8, 5))]),
        ("[C]", counter(None, None, None)),
        ("[C10]", counter(Some(10), None, None)),
        ("[C10+5:3]", counter(Some(10), Some(5), digits(3))),
        ("[C+5]", counter(None, Some(5), None)),
        ("[C:3]", counter(None, None, digits(3))),
        ("[C:a]", counter(None, None, Some(CounterWidth::Auto))),
        (
            "[C10+10:a]",
            counter(Some(10), Some(10), Some(CounterWidth::Auto)),
        ),
        ("[C-5+-1]", counter(Some(-5), Some(-1), None)),
        ("[Y]", dates(&[DatePart::Year])),
        ("[y]", dates(&[DatePart::Year2])),
        ("[M]", dates(&[DatePart::Month])),
        ("[D]", dates(&[DatePart::Day])),
        ("[h]", dates(&[DatePart::Hour])),
        ("[H]", dates(&[DatePart::Hour12])),
        ("[m]", dates(&[DatePart::Minute])),
        ("[s]", dates(&[DatePart::Second])),
        ("[i]", dates(&[DatePart::AmPm])),
        ("[i1]", dates(&[DatePart::AmPmShort])),
        (
            "[YMD]",
            dates(&[DatePart::Year, DatePart::Month, DatePart::Day]),
        ),
        (
            "[hms]",
            dates(&[DatePart::Hour, DatePart::Minute, DatePart::Second]),
        ),
        ("[T1]", vec![]),
        ("[U]", vec![Token::Case(CaseMode::Upper)]),
        ("[L]", vec![Token::Case(CaseMode::Lower)]),
        ("[F]", vec![Token::Case(CaseMode::WordsUpper)]),
        ("[n]", vec![Token::Case(CaseMode::Unchanged)]),
        ("[[]", vec![Token::Literal("[".to_owned())]),
        ("[]]", vec![Token::Literal("]".to_owned())]),
    ];
    for (mask, expected) in cases {
        assert_eq!(tokens(mask), expected, "{mask}");
    }
}

#[test]
fn literal_text_and_placeholders_alternate() {
    assert_eq!(
        tokens("IMG_[C:3] ([N]) [[]x[]]"),
        [
            Token::Literal("IMG_".to_owned()),
            Token::Counter(CounterSpec {
                start: None,
                step: None,
                width: Some(CounterWidth::Digits(3)),
            }),
            Token::Literal(" (".to_owned()),
            Token::Name(Range::All),
            Token::Literal(") [x]".to_owned()),
        ]
    );
    assert!(tokens("").is_empty());
}

#[test]
fn a_bad_placeholder_is_an_error_at_its_position() {
    let unknown = |text: &str| PatternErrorKind::Unknown {
        text: text.to_owned(),
    };
    let arguments = |text: &str| PatternErrorKind::Arguments {
        text: text.to_owned(),
    };
    let cases = [
        ("abc[N", 3, PatternErrorKind::Unclosed),
        ("[N[E]", 0, PatternErrorKind::Unclosed),
        ("ab]", 2, PatternErrorKind::StrayBracket),
        ("x[]", 1, PatternErrorKind::Empty),
        ("[X]", 0, unknown("X")),
        ("[m]_[Yq]", 4, unknown("Yq")),
        // Total Commander placeholders rCommander does not offer (yet).
        ("[d]", 0, unknown("d")),
        ("[t]", 0, unknown("t")),
        ("[S]", 0, unknown("S")),
        ("[R]", 0, unknown("R")),
        ("[T2]", 0, unknown("T2")),
        ("[=plugin.field]", 0, unknown("=plugin.field")),
        ("[2-5]", 0, unknown("2-5")),
        // Double Commander's forms are not Total Commander's.
        ("[V:hint]", 0, unknown("V:hint")),
        ("[N2:5]", 0, arguments("N2:5")),
        ("[N0]", 0, arguments("N0")),
        ("[N2-0]", 0, arguments("N2-0")),
        ("[N1,0]", 0, arguments("N1,0")),
        ("[N+1]", 0, arguments("N+1")),
        ("[N-]", 0, arguments("N-")),
        ("[N2-5x]", 0, arguments("N2-5x")),
        ("[E1-x]", 0, arguments("E1-x")),
        ("[Px]", 0, arguments("Px")),
        ("[C+]", 0, arguments("C+")),
        ("[C:x]", 0, arguments("C:x")),
        ("[C:33]", 0, arguments("C:33")),
        ("[Caa+1]", 0, arguments("Caa+1")),
        ("[C+1/100]", 0, arguments("C+1/100")),
        // Positions count characters, not bytes.
        ("\u{e4}\u{f6}[Q]", 2, unknown("Q")),
    ];
    for (mask, position, kind) in cases {
        assert_eq!(parse(mask), Err(PatternError { position, kind }), "{mask}");
    }
}
