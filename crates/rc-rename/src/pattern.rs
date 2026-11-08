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
//! - `[B0]` ... `[B9]` the parent, grandparent, great-grandparent folder and so on; `[B+0]` ...
//!   `[B+9]` the path from the root: the drive (or server), the first folder, the second; each
//!   takes a range after `_`, `[B0_2-5]`;
//! - `[C]` the counter with the rule's settings, `[C10+5:3]` with its own (start, step, digits;
//!   each part optional), `:a` for as many digits as the last number needs, `[C+1/100]` a step
//!   of a hundredth (the whole part: `[C+1/100]\[N]` puts every 100 entries in a subfolder);
//!   `[Caa+1]` counts in letters from `aa` (`[CAAA]` upper case; after `zz` comes `aaa`);
//! - `[Y]` year, `[y]` two-digit year, `[M]` month, `[D]` day, `[h]` hour (24), `[H]` hour (12),
//!   `[m]` minute, `[s]` second, `[i]` am/pm, `[i1]` a/p; several in one bracket, `[YMD]`,
//!   `[hms]`; `[d]` and `[t]` the date and the time in the local format, `/` as `-` and `:` as
//!   `.`; they read the modification time, or after `[T2]` the time the dialog was opened,
//!   after `[T3]` the time the options last changed, `[T1]` the modification time again - each
//!   with an optional offset, `[T1+60m]` (`s`, `m`, `h`, `D`, `M`, `Y`);
//! - `[S]` the size in bytes (a folder's is 0); `[X]` the clipboard's text, `*?:|"<>` as `_`,
//!   an error when it is empty, `[X+]` nothing then;
//! - `[I]` or `[I1]` ignore the dots in folder names from here on (`[N]` the whole name, `[E]`
//!   empty), `[I0]` back;
//! - `[U]`, `[L]`, `[F]` upper case, lower case, first letter of each word upper case from here
//!   on, `[f]` the same with English title words (`of`, `the`, ...) lower case but at the start
//!   and the end, `[n]` the case as rendered again;
//! - `[[]` and `[]]` a literal `[` and `]`.
//!
//! The switches `[T...]` and `[I]` carry from the name mask into the extension mask. Anything
//! else in brackets is an error with its position, never silently literal text; Total
//! Commander's `[T4]` (EXIF) and `[=plugin.field]` among them.

use crate::args::{counter, letters, path_level, range, time_switch};
use crate::case::CaseMode;
use crate::clock::TimeSwitch;
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
    /// `[B0...]` ... `[B9...]`: the folder that many levels above the parent (0 the parent).
    Ancestor(u8, Range),
    /// `[B+0...]` ... `[B+9...]`: a part of the path from the root (0 the drive or server).
    FromRoot(u8, Range),
    /// `[C...]`: the counter.
    Counter(CounterSpec),
    /// `[Caa+1]`: the counter in letters.
    Letters(LetterCounter),
    /// A part of the time the switches choose.
    Date(DatePart),
    /// `[d]`: the date in the local format.
    LocalDate,
    /// `[t]`: the time in the local format.
    LocalTime,
    /// `[S]`: the size in bytes.
    Size,
    /// `[X]` (`optional: false`) and `[X+]`: the clipboard's text.
    Clipboard { optional: bool },
    /// `[U]`, `[L]`, `[F]`, `[f]`, `[n]`: the case of everything after it.
    Case(CaseMode),
    /// `[I]`, `[I1]` (on) and `[I0]` (off): ignore the dots in folder names.
    IgnoreFolderDots(bool),
    /// `[T1]`, `[T2]`, `[T3]` with an optional offset: the time the date placeholders read.
    Time(TimeSwitch),
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
    /// `[C+1/100]`: the step is divided by this, and the counter shows the whole part.
    pub divisor: Option<u32>,
}

/// A counter in letters, `[Caa+1]`: `aa`, `ab`, ... `zz`, `aaa`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LetterCounter {
    /// The first value, `a` being 0: what the letters spell.
    pub start: u64,
    /// The least number of letters: as many as the mask has.
    pub width: u32,
    /// `[CAA]`: upper case letters.
    pub upper: bool,
    /// What each further entry adds; the rule's step when `None`.
    pub step: Option<i64>,
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
    let unknown = || PatternErrorKind::Unknown {
        text: body.to_owned(),
    };
    let token = match body {
        "" => return Err(PatternErrorKind::Empty),
        "U" => Token::Case(CaseMode::Upper),
        "L" => Token::Case(CaseMode::Lower),
        "F" => Token::Case(CaseMode::WordsUpper),
        "f" => Token::Case(CaseMode::Title),
        "n" => Token::Case(CaseMode::Unchanged),
        "i1" => Token::Date(DatePart::AmPmShort),
        "d" => Token::LocalDate,
        "t" => Token::LocalTime,
        "S" => Token::Size,
        "X" => Token::Clipboard { optional: false },
        "X+" => Token::Clipboard { optional: true },
        "I" | "I1" => Token::IgnoreFolderDots(true),
        "I0" => Token::IgnoreFolderDots(false),
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
                Some('C') if rest.starts_with(|c: char| c.is_ascii_alphabetic()) => {
                    Token::Letters(letters(rest).ok_or_else(arguments)?)
                }
                Some('C') => Token::Counter(counter(rest).ok_or_else(arguments)?),
                Some('B') => match rest.strip_prefix('+') {
                    Some(rest) => {
                        let (level, range) = path_level(rest).ok_or_else(arguments)?;
                        Token::FromRoot(level, range)
                    }
                    None => {
                        let (level, range) = path_level(rest).ok_or_else(arguments)?;
                        Token::Ancestor(level, range)
                    }
                },
                // `[T4]`, the EXIF date, is not offered.
                Some('T') if rest.starts_with('4') => return Err(unknown()),
                Some('T') => Token::Time(time_switch(rest).ok_or_else(arguments)?),
                _ => return Err(unknown()),
            }
        }
    };
    tokens.push(token);
    Ok(())
}

#[cfg(test)]
#[path = "pattern_tests.rs"]
mod tests;
