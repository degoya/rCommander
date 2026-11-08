use super::{CounterSpec, CounterWidth, DatePart, LetterCounter, Range, Token, parse};
use crate::case::CaseMode;
use crate::clock::{OffsetUnit, TimeOffset, TimeSource, TimeSwitch};
use crate::error::{PatternError, PatternErrorKind};

fn tokens(mask: &str) -> Vec<Token> {
    parse(mask)
        .unwrap_or_else(|error| panic!("{mask}: {error}"))
        .tokens()
        .to_vec()
}

#[test]
fn every_placeholder_parses_to_its_tokens() {
    let counter = |start, step, width| {
        vec![Token::Counter(CounterSpec {
            start,
            step,
            width,
            divisor: None,
        })]
    };
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
        ("[T1]", vec![Token::Time(TimeSwitch::default())]),
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
                divisor: None,
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
        ("[Q]", 0, unknown("Q")),
        ("[m]_[Yq]", 4, unknown("Yq")),
        // Total Commander placeholders rCommander does not offer; `[R]` is not Total
        // Commander's at all.
        ("[R]", 0, unknown("R")),
        ("[T4]", 0, unknown("T4")),
        ("[T4+1D]", 0, unknown("T4+1D")),
        ("[=plugin.field]", 0, unknown("=plugin.field")),
        ("[X-]", 0, unknown("X-")),
        ("[I2]", 0, unknown("I2")),
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
        ("[CaA]", 0, arguments("CaA")),
        ("[Caa+]", 0, arguments("Caa+")),
        ("[Caa:3]", 0, arguments("Caa:3")),
        ("[Caaaaaaaaaaaaa]", 0, arguments("Caaaaaaaaaaaaa")),
        ("[C+1/0]", 0, arguments("C+1/0")),
        ("[C+1/]", 0, arguments("C+1/")),
        ("[C/100]", 0, arguments("C/100")),
        ("[B]", 0, arguments("B")),
        ("[B10]", 0, arguments("B10")),
        ("[B+]", 0, arguments("B+")),
        ("[B0_]", 0, arguments("B0_")),
        ("[B0-2]", 0, arguments("B0-2")),
        ("[T]", 0, arguments("T")),
        ("[T5]", 0, arguments("T5")),
        ("[T1+60]", 0, arguments("T1+60")),
        ("[T1+60x]", 0, arguments("T1+60x")),
        ("[T1*60m]", 0, arguments("T1*60m")),
        ("[T1+9999999m]", 0, arguments("T1+9999999m")),
        // Positions count characters, not bytes.
        ("\u{e4}\u{f6}[Q]", 2, unknown("Q")),
    ];
    for (mask, position, kind) in cases {
        assert_eq!(parse(mask), Err(PatternError { position, kind }), "{mask}");
    }
}

#[test]
fn the_placeholders_of_rc_100_96_parse() {
    let switch = |source, offset| vec![Token::Time(TimeSwitch { source, offset })];
    let offset = |amount, unit| Some(TimeOffset { amount, unit });
    let letters = |start, width, upper, step| {
        vec![Token::Letters(LetterCounter {
            start,
            width,
            upper,
            step,
        })]
    };
    let cases: Vec<(&str, Vec<Token>)> = vec![
        ("[S]", vec![Token::Size]),
        ("[f]", vec![Token::Case(CaseMode::Title)]),
        ("[I]", vec![Token::IgnoreFolderDots(true)]),
        ("[I1]", vec![Token::IgnoreFolderDots(true)]),
        ("[I0]", vec![Token::IgnoreFolderDots(false)]),
        ("[X]", vec![Token::Clipboard { optional: false }]),
        ("[X+]", vec![Token::Clipboard { optional: true }]),
        ("[d]", vec![Token::LocalDate]),
        ("[t]", vec![Token::LocalTime]),
        ("[Caa+1]", letters(0, 2, false, Some(1))),
        ("[CAAA]", letters(0, 3, true, None)),
        ("[Cba]", letters(26, 2, false, None)),
        ("[Cz+-2]", letters(25, 1, false, Some(-2))),
        (
            "[C+1/100]",
            vec![Token::Counter(CounterSpec {
                start: None,
                step: Some(1),
                width: None,
                divisor: Some(100),
            })],
        ),
        (
            "[C5+2/10:3]",
            vec![Token::Counter(CounterSpec {
                start: Some(5),
                step: Some(2),
                width: Some(CounterWidth::Digits(3)),
                divisor: Some(10),
            })],
        ),
        ("[T2]", switch(TimeSource::Opened, None)),
        ("[T3]", switch(TimeSource::Changed, None)),
        (
            "[T1+60m]",
            switch(TimeSource::Modified, offset(60, OffsetUnit::Minutes)),
        ),
        (
            "[T2-1D]",
            switch(TimeSource::Opened, offset(-1, OffsetUnit::Days)),
        ),
        (
            "[T3+2Y]",
            switch(TimeSource::Changed, offset(2, OffsetUnit::Years)),
        ),
        (
            "[T1+5M]",
            switch(TimeSource::Modified, offset(5, OffsetUnit::Months)),
        ),
        (
            "[T1+5s]",
            switch(TimeSource::Modified, offset(5, OffsetUnit::Seconds)),
        ),
        (
            "[T1+5h]",
            switch(TimeSource::Modified, offset(5, OffsetUnit::Hours)),
        ),
        ("[B0]", vec![Token::Ancestor(0, Range::All)]),
        ("[B9]", vec![Token::Ancestor(9, Range::All)]),
        (
            "[B0_2-5]",
            vec![Token::Ancestor(0, Range::Span(2, Some(5)))],
        ),
        ("[B+0]", vec![Token::FromRoot(0, Range::All)]),
        (
            "[B+1_2-5]",
            vec![Token::FromRoot(1, Range::Span(2, Some(5)))],
        ),
    ];
    for (mask, expected) in cases {
        assert_eq!(tokens(mask), expected, "{mask}");
    }
}

#[test]
fn r_is_an_unknown_placeholder() {
    assert_eq!(
        parse("[N]_[R]"),
        Err(PatternError {
            position: 4,
            kind: PatternErrorKind::Unknown {
                text: "R".to_owned(),
            },
        })
    );
}
