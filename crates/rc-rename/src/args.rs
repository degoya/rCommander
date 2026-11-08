//! The arguments inside a placeholder: ranges, counters, path levels and time switches. Each
//! parser returns `None` for arguments it does not take; the mask parser turns that into
//! `rename_mask_arguments` with the placeholder's position.

use crate::clock::{MAX_OFFSET, OffsetUnit, TimeOffset, TimeSource, TimeSwitch};
use crate::pattern::{CounterSpec, CounterWidth, LetterCounter, MAX_COUNTER_WIDTH, Range};

/// The most letters a letter counter may have: 26^12 values still fit in 64 bits.
pub(crate) const MAX_LETTERS: u32 = 12;

/// The largest divisor of a fractional counter (`[C+1/100]`).
pub(crate) const MAX_DIVISOR: u32 = 1_000_000;

/// The range after `N`, `E`, `A`, `P` or `G`: nothing, `x`, `x-y`, `x-`, `x,n`. After a
/// negative start the end counts from the end too (`[N-8-5]`); after a positive one it does
/// only with its own minus (`[N2--5]`).
pub(crate) fn range(text: &str) -> Option<Range> {
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

/// `[start][+step[/divisor]][:width|:a]` after `C`.
pub(crate) fn counter(text: &str) -> Option<CounterSpec> {
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
    let (start, step, divisor) = match rest.split_once('+') {
        Some((start, step)) => match step.split_once('/') {
            Some((step, divisor)) => {
                let divisor = u32::try_from(unsigned(divisor)?)
                    .ok()
                    .filter(|divisor| (1..=MAX_DIVISOR).contains(divisor))?;
                (start, Some(signed(step)?), Some(divisor))
            }
            None => (start, Some(signed(step)?), None),
        },
        None => (rest, None, None),
    };
    let start = if start.is_empty() {
        None
    } else {
        Some(signed(start)?)
    };
    Some(CounterSpec {
        start,
        step,
        width,
        divisor,
    })
}

/// `letters[+step]` after `C`: `[Caa+1]`, `[CAAA]`. The letters are the first value and the
/// width, all lower or all upper case.
pub(crate) fn letters(text: &str) -> Option<LetterCounter> {
    let end = text
        .find(|c: char| !c.is_ascii_alphabetic())
        .unwrap_or(text.len());
    let (letters, rest) = text.split_at(end);
    let upper = letters.bytes().all(|byte| byte.is_ascii_uppercase());
    if !upper && !letters.bytes().all(|byte| byte.is_ascii_lowercase()) {
        return None;
    }
    let width = u32::try_from(letters.len())
        .ok()
        .filter(|width| (1..=MAX_LETTERS).contains(width))?;
    let start = letters.bytes().fold(0_u64, |value, byte| {
        value * 26 + u64::from(byte.to_ascii_lowercase() - b'a')
    });
    let step = if rest.is_empty() {
        None
    } else {
        Some(signed(rest.strip_prefix('+')?)?)
    };
    Some(LetterCounter {
        start,
        width,
        upper,
        step,
    })
}

/// `digit[_range]` after `B` or `B+`: the level, 0 to 9, and the characters taken.
pub(crate) fn path_level(text: &str) -> Option<(u8, Range)> {
    let mut chars = text.chars();
    let level = u8::try_from(chars.next()?.to_digit(10)?).ok()?;
    let rest = chars.as_str();
    if rest.is_empty() {
        return Some((level, Range::All));
    }
    let wanted = rest.strip_prefix('_').filter(|wanted| !wanted.is_empty())?;
    Some((level, range(wanted)?))
}

/// `1`, `2` or `3` and an optional offset (`+60m`, `-1D`) after `T`.
pub(crate) fn time_switch(text: &str) -> Option<TimeSwitch> {
    let mut chars = text.chars();
    let source = match chars.next()? {
        '1' => TimeSource::Modified,
        '2' => TimeSource::Opened,
        '3' => TimeSource::Changed,
        _ => return None,
    };
    let rest = chars.as_str();
    if rest.is_empty() {
        return Some(TimeSwitch {
            source,
            offset: None,
        });
    }
    let (negative, rest) = match rest.strip_prefix('+') {
        Some(rest) => (false, rest),
        None => (true, rest.strip_prefix('-')?),
    };
    let unit_letter = rest.chars().next_back()?;
    let unit = OffsetUnit::from_letter(unit_letter)?;
    let amount = unsigned(&rest[..rest.len() - unit_letter.len_utf8()])
        .filter(|amount| *amount <= MAX_OFFSET)?;
    Some(TimeSwitch {
        source,
        offset: Some(TimeOffset {
            amount: if negative { -amount } else { amount },
            unit,
        }),
    })
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
