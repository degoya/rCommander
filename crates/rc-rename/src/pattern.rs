//! The mask parser: literal text and Total Commander's placeholders.
//!
//! Total Commander 11 is the reference (its Multi-Rename help); the placeholders are
//! case-sensitive:
//!
//! - `[N]` the name without the extension, `[E]` the extension, `[A]` the name with the
//!   extension, `[P]` the parent folder, `[G]` the grandparent folder; each takes a range:
//!   `[N1]` one character, `[N2-5]` characters 2 to 5, `[N2,5]` five characters from 2,
//!   `[N2-]` from 2 to the end, `[N-8,5]` five from the 8th-last, `[N-8-5]` from the 8th-last to
//!   the 5th-last, `[N2--5]` from 2 to the 5th-last, `[N-5-]` from the 5th-last to the end;
//! - `[C]` the counter with the rule's settings, `[C10+5:3]` with its own (start, step, digits;
//!   each part optional), `:a` for as many digits as the last number needs;
//! - `[Y]` year, `[y]` two-digit year, `[M]` month, `[D]` day, `[h]` hour (24), `[H]` hour (12),
//!   `[m]` minute, `[s]` second, `[i]` am/pm, `[i1]` a/p - the modification time; several in one
//!   bracket, `[YMD]`, `[hms]`; `[T1]` (modification time, the default) is accepted;
//! - `[U]`, `[L]`, `[F]` upper case, lower case, first letter of each word upper case from here
//!   on, `[n]` the case as rendered again;
//! - `[[]` and `[]]` a literal `[` and `]`.
//!
//! Anything else in brackets is an error with its position, never silently literal text.

use crate::case::CaseMode;
use crate::error::{PatternError, PatternErrorKind};

/// A parsed mask.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Pattern {
    tokens: Vec<Token>,
}

impl Pattern {
    /// The literal texts and placeholders in order.
    #[must_use]
    pub fn tokens(&self) -> &[Token] {
        &self.tokens
    }
}

/// One piece of a [`Pattern`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Token {
    /// Text taken as it is (with `[[]` and `[]]` already resolved).
    Literal(String),
    /// `[N...]`: the name without the extension.
    Name(Range),
    /// `[E...]`: the extension, without the dot.
    Extension(Range),
    /// `[A...]`: the name with the extension.
    FileName(Range),
    /// `[P...]`: the parent folder's name.
    Parent(Range),
    /// `[G...]`: the grandparent folder's name.
    Grandparent(Range),
    /// `[C...]`: the counter.
    Counter(CounterSpec),
    /// A part of the modification time.
    Date(DatePart),
    /// `[U]`, `[L]`, `[F]`, `[n]`: the case of everything after it.
    Case(CaseMode),
}

/// Which characters a placeholder takes. Positions count from 1; a negative position counts
/// from the end (`-1` is the last character). A range that lies outside the text takes what is
/// inside, a range that ends before it starts takes nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Range {
    /// The whole text.
    All,
    /// One character: `[N3]`, `[N-3]`.
    At(i64),
    /// From one position to another or (`None`) to the end: `[N2-5]`, `[N2-]`, `[N-8-5]`.
    Span(i64, Option<i64>),
    /// A number of characters from a position, rightwards: `[N2,5]`, `[N-8,5]`.
    Count(i64, u64),
}

/// The settings of one `[C...]`; what is `None` comes from the rule's counter settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CounterSpec {
    /// The value for the first entry.
    pub start: Option<i64>,
    /// What each further entry adds.
    pub step: Option<i64>,
    /// The number of digits.
    pub width: Option<CounterWidth>,
}

/// How many digits a counter has.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CounterWidth {
    /// At least this many, padded with zeros.
    Digits(u32),
    /// `:a`: as many as the run's last value needs.
    Auto,
}

/// The widest counter a mask or a rule may ask for.
pub(crate) const MAX_COUNTER_WIDTH: u32 = 32;

/// A part of the modification time, always with leading zeros.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DatePart {
    /// `[Y]`: the year, four digits.
    Year,
    /// `[y]`: the year, two digits.
    Year2,
    /// `[M]`: the month.
    Month,
    /// `[D]`: the day of the month.
    Day,
    /// `[h]`: the hour, 00 to 23.
    Hour,
    /// `[H]`: the hour, 01 to 12.
    Hour12,
    /// `[m]`: the minute.
    Minute,
    /// `[s]`: the second.
    Second,
    /// `[i]`: `am` or `pm`.
    AmPm,
    /// `[i1]`: `a` or `p`.
    AmPmShort,
}

impl DatePart {
    fn from_letter(letter: char) -> Option<Self> {
        Some(match letter {
            'Y' => Self::Year,
            'y' => Self::Year2,
            'M' => Self::Month,
            'D' => Self::Day,
            'h' => Self::Hour,
            'H' => Self::Hour12,
            'm' => Self::Minute,
            's' => Self::Second,
            'i' => Self::AmPm,
            _ => return None,
        })
    }
}

/// Parses a name or extension mask.
///
/// # Errors
///
/// A [`PatternError`] with the position of the first placeholder that does not parse.
pub fn parse(mask: &str) -> Result<Pattern, PatternError> {
    let chars: Vec<char> = mask.chars().collect();
    let mut tokens = Vec::new();
    let mut literal = String::new();
    let mut at = 0;
    while let Some(&c) = chars.get(at) {
        match c {
            '[' => {
                let next = (chars.get(at + 1).copied(), chars.get(at + 2).copied());
                if let (Some(bracket @ ('[' | ']')), Some(']')) = next {
                    literal.push(bracket);
                    at += 3;
                    continue;
                }
                let error = |kind| PatternError { position: at, kind };
                let close = chars[at + 1..]
                    .iter()
                    .position(|&c| c == ']')
                    .map(|offset| at + 1 + offset)
                    .ok_or_else(|| error(PatternErrorKind::Unclosed))?;
                let body: String = chars[at + 1..close].iter().collect();
                if body.contains('[') {
                    return Err(error(PatternErrorKind::Unclosed));
                }
                if !literal.is_empty() {
                    tokens.push(Token::Literal(std::mem::take(&mut literal)));
                }
                placeholder(&body, &mut tokens).map_err(error)?;
                at = close + 1;
            }
            ']' => {
                return Err(PatternError {
                    position: at,
                    kind: PatternErrorKind::StrayBracket,
                });
            }
            c => {
                literal.push(c);
                at += 1;
            }
        }
    }
    if !literal.is_empty() {
        tokens.push(Token::Literal(literal));
    }
    Ok(Pattern { tokens })
}

/// Appends the tokens for the text between `[` and `]`.
fn placeholder(body: &str, tokens: &mut Vec<Token>) -> Result<(), PatternErrorKind> {
    let arguments = || PatternErrorKind::Arguments {
        text: body.to_owned(),
    };
    let token = match body {
        "" => return Err(PatternErrorKind::Empty),
        // The modification time is the default time source already.
        "T1" => return Ok(()),
        "U" => Token::Case(CaseMode::Upper),
        "L" => Token::Case(CaseMode::Lower),
        "F" => Token::Case(CaseMode::WordsUpper),
        "n" => Token::Case(CaseMode::Unchanged),
        "i1" => Token::Date(DatePart::AmPmShort),
        _ => {
            let dates: Option<Vec<DatePart>> = body.chars().map(DatePart::from_letter).collect();
            if let Some(dates) = dates {
                tokens.extend(dates.into_iter().map(Token::Date));
                return Ok(());
            }
            let mut chars = body.chars();
            let head = chars.next();
            let rest = chars.as_str();
            match head {
                Some('N') => Token::Name(range(rest).ok_or_else(arguments)?),
                Some('E') => Token::Extension(range(rest).ok_or_else(arguments)?),
                Some('A') => Token::FileName(range(rest).ok_or_else(arguments)?),
                Some('P') => Token::Parent(range(rest).ok_or_else(arguments)?),
                Some('G') => Token::Grandparent(range(rest).ok_or_else(arguments)?),
                Some('C') => Token::Counter(counter(rest).ok_or_else(arguments)?),
                _ => {
                    return Err(PatternErrorKind::Unknown {
                        text: body.to_owned(),
                    });
                }
            }
        }
    };
    tokens.push(token);
    Ok(())
}

/// The range after `N`, `E`, `A`, `P` or `G`: nothing, `x`, `x-y`, `x-`, `x,n`. After a
/// negative start the end counts from the end too (`[N-8-5]`); after a positive one it does
/// only with its own minus (`[N2--5]`).
fn range(text: &str) -> Option<Range> {
    if text.is_empty() {
        return Some(Range::All);
    }
    let (from, rest) = leading_position(text)?;
    if rest.is_empty() {
        return Some(Range::At(from));
    }
    if let Some(count) = rest.strip_prefix(',') {
        let count = u64::try_from(unsigned(count)?).ok()?;
        return (count > 0).then_some(Range::Count(from, count));
    }
    let to = rest.strip_prefix('-')?;
    if to.is_empty() {
        return Some(Range::Span(from, None));
    }
    let (to, rest) = leading_position(to)?;
    if !rest.is_empty() {
        return None;
    }
    let to = if from < 0 && to > 0 { -to } else { to };
    Some(Range::Span(from, Some(to)))
}

/// A position at the start of `text` - an optional `-` and digits, not 0 - and what follows.
fn leading_position(text: &str) -> Option<(i64, &str)> {
    let (negative, digits) = match text.strip_prefix('-') {
        Some(digits) => (true, digits),
        None => (false, text),
    };
    let end = digits
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(digits.len());
    let value = unsigned(&digits[..end]).filter(|value| *value != 0)?;
    Some((if negative { -value } else { value }, &digits[end..]))
}

/// `[start][+step][:width|:a]` after `C`.
fn counter(text: &str) -> Option<CounterSpec> {
    let (rest, width) = match text.split_once(':') {
        Some((rest, "a")) => (rest, Some(CounterWidth::Auto)),
        Some((rest, width)) => {
            let width = u32::try_from(unsigned(width)?)
                .ok()
                .filter(|width| *width <= MAX_COUNTER_WIDTH)?;
            (rest, Some(CounterWidth::Digits(width)))
        }
        None => (text, None),
    };
    let (start, step) = match rest.split_once('+') {
        Some((start, step)) => (start, Some(signed(step)?)),
        None => (rest, None),
    };
    let start = if start.is_empty() {
        None
    } else {
        Some(signed(start)?)
    };
    Some(CounterSpec { start, step, width })
}

/// An optional `-` and decimal digits.
fn signed(text: &str) -> Option<i64> {
    match text.strip_prefix('-') {
        Some(digits) => unsigned(digits).map(|value| -value),
        None => unsigned(text),
    }
}

/// Decimal digits only, no sign.
fn unsigned(text: &str) -> Option<i64> {
    if text.is_empty() || !text.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    text.parse().ok()
}

#[cfg(test)]
#[path = "pattern_tests.rs"]
mod tests;
