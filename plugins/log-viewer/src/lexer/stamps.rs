//! Timestamps, coloured `number`: ISO 8601 (`2026-10-20T15:20:01.123+02:00`, also with a space,
//! a `,` fraction as Python writes it, or `/` as nginx's error log), a clock (`15:20:01.123`),
//! syslog and ctime (`Oct 20 15:20:01`, `Oct 20 15:20:01.123 2026`), Apache
//! (`20/Oct/2026:15:20:01 +0200`), and at a line's start Unix seconds (`1792502401.5`), logcat's
//! `10-20 15:20:01.123` and glog's `E1020 15:20:01.123456`.
//!
//! Each matcher returns where the timestamp ends, or `None`; it looks at most a few dozen bytes
//! ahead. A timestamp starts and ends at a word's edge: `x2026-10-20` and `15:20:01x` are none.

use super::is_word;

const MONTHS: [&[u8]; 12] = [
    b"Jan", b"Feb", b"Mar", b"Apr", b"May", b"Jun", b"Jul", b"Aug", b"Sep", b"Oct", b"Nov", b"Dec",
];

/// A timestamp that starts with a digit at `at`: ISO 8601, Apache or a clock.
pub(super) fn at_digit(line: &[u8], at: usize) -> Option<usize> {
    if !starts_word(line, at) {
        return None;
    }
    iso(line, at)
        .or_else(|| apache(line, at))
        .or_else(|| clock(line, at))
        .filter(|end| ends_word(line, *end))
}

/// Any timestamp at `at` but the line-start forms: what a quoted string may hold whole.
pub(super) fn any(line: &[u8], at: usize) -> Option<usize> {
    at_digit(line, at).or_else(|| syslog(line, at))
}

/// syslog's and ctime's `Oct 20 15:20:01`, the day padded with a space or not, with a fraction
/// and a year after it or not.
pub(super) fn syslog(line: &[u8], at: usize) -> Option<usize> {
    if !starts_word(line, at) || !MONTHS.contains(&line.get(at..at + 3)?) {
        return None;
    }
    let mut end = at + 3;
    end += spaces(line, end, 2)?;
    let day = digit_run(line, end);
    if !(1..=2).contains(&day) || byte(line, end + day) != Some(b' ') {
        return None;
    }
    end = clock(line, end + day + 1)?;
    if byte(line, end) == Some(b' ') && digits(line, end + 1, 4) && ends_word(line, end + 5) {
        end += 5;
    }
    ends_word(line, end).then_some(end)
}

/// Unix seconds at `at`, a line's start or just after its `[`: ten digits, a fraction or not.
pub(super) fn unix_seconds(line: &[u8], at: usize) -> Option<usize> {
    if digit_run(line, at) != 10 {
        return None;
    }
    let end = fraction(line, at + 10);
    ends_word(line, end).then_some(end)
}

/// logcat's `10-20 15:20:01.123` at the line's start.
pub(super) fn logcat(line: &[u8]) -> Option<usize> {
    if !(digits(line, 0, 2) && byte(line, 2) == Some(b'-') && digits(line, 3, 2)) {
        return None;
    }
    if byte(line, 5) != Some(b' ') {
        return None;
    }
    clock(line, 6).filter(|end| ends_word(line, *end))
}

/// glog's `E1020 15:20:01.123456` at the line's start: where the timestamp after the level letter
/// ends. The caller checks the letter.
pub(super) fn glog(line: &[u8]) -> Option<usize> {
    if !(digits(line, 1, 4) && byte(line, 5) == Some(b' ')) {
        return None;
    }
    clock(line, 6).filter(|end| ends_word(line, *end))
}

/// `2026-10-20`, `2026/10/20`, then a time after `T` or a space, then a zone.
fn iso(line: &[u8], at: usize) -> Option<usize> {
    let separator = byte(line, at + 4).filter(|byte| matches!(byte, b'-' | b'/'))?;
    let date = digits(line, at, 4)
        && digits(line, at + 5, 2)
        && byte(line, at + 7) == Some(separator)
        && digits(line, at + 8, 2);
    if !date {
        return None;
    }
    let end = at + 10;
    if !matches!(byte(line, end), Some(b'T' | b' ')) {
        return Some(end);
    }
    Some(time(line, end + 1).map_or(end, |time| zone(line, time)))
}

/// Apache's `20/Oct/2026:15:20:01 +0200`.
fn apache(line: &[u8], at: usize) -> Option<usize> {
    let date = digits(line, at, 2)
        && byte(line, at + 2) == Some(b'/')
        && MONTHS.contains(&line.get(at + 3..at + 6)?)
        && byte(line, at + 6) == Some(b'/')
        && digits(line, at + 7, 4)
        && byte(line, at + 11) == Some(b':');
    if !date {
        return None;
    }
    let end = clock(line, at + 12)?;
    let offset = byte(line, end) == Some(b' ')
        && matches!(byte(line, end + 1), Some(b'+' | b'-'))
        && digits(line, end + 2, 4);
    Some(if offset { end + 6 } else { end })
}

/// `15:20:01`, a fraction or not.
fn clock(line: &[u8], at: usize) -> Option<usize> {
    let end = time(line, at)?;
    (end >= at + 8).then_some(end)
}

/// `15:20`, `15:20:01`, `15:20:01.123` or `15:20:01,123`.
fn time(line: &[u8], at: usize) -> Option<usize> {
    if !(digits(line, at, 2) && byte(line, at + 2) == Some(b':') && digits(line, at + 3, 2)) {
        return None;
    }
    if !(byte(line, at + 5) == Some(b':') && digits(line, at + 6, 2)) {
        return Some(at + 5);
    }
    Some(fraction(line, at + 8))
}

/// After the seconds: `.123` or `,123`, nine digits at most; `end` when there is none.
fn fraction(line: &[u8], end: usize) -> usize {
    let run = digit_run(line, end + 1);
    if matches!(byte(line, end), Some(b'.' | b',')) && (1..=9).contains(&run) {
        end + 1 + run
    } else {
        end
    }
}

/// After the time: `Z`, `+02:00`, `+0200` or `-07`.
fn zone(line: &[u8], end: usize) -> usize {
    match byte(line, end) {
        Some(b'Z') => end + 1,
        Some(b'+' | b'-') if digits(line, end + 1, 2) => {
            if digits(line, end + 3, 2) {
                end + 5
            } else if byte(line, end + 3) == Some(b':') && digits(line, end + 4, 2) {
                end + 6
            } else {
                end + 3
            }
        }
        _ => end,
    }
}

/// Up to `most` spaces at `at`, at least one: how many.
fn spaces(line: &[u8], at: usize, most: usize) -> Option<usize> {
    let count = line
        .get(at..)?
        .iter()
        .take(most)
        .take_while(|byte| **byte == b' ')
        .count();
    (count > 0).then_some(count)
}

fn byte(line: &[u8], at: usize) -> Option<u8> {
    line.get(at).copied()
}

/// Whether `count` digits stand at `at`.
fn digits(line: &[u8], at: usize, count: usize) -> bool {
    line.get(at..at + count)
        .is_some_and(|run| run.iter().all(u8::is_ascii_digit))
}

/// How many digits stand at `at`; at most 16 are counted, so a long number costs no more.
fn digit_run(line: &[u8], at: usize) -> usize {
    line.get(at..)
        .unwrap_or_default()
        .iter()
        .take(16)
        .take_while(|byte| byte.is_ascii_digit())
        .count()
}

fn starts_word(line: &[u8], at: usize) -> bool {
    at == 0 || !line.get(at - 1).copied().is_some_and(is_word)
}

fn ends_word(line: &[u8], end: usize) -> bool {
    !line.get(end).copied().is_some_and(is_word)
}

#[cfg(test)]
#[path = "stamps_tests.rs"]
mod tests;
