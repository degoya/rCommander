//! Addresses, coloured `literal`: IPv4 (with a port), IPv6, URLs, Windows paths (`C:\x`, `C:/x`,
//! `\\server\share`) and Unix paths (`/var/log`, `./x`, `../x`, `~/x`).
//!
//! Each matcher returns where the address ends, or `None`. An IP address looks at most 46 bytes
//! ahead; a URL or a path runs to the next space or quote and is consumed whole, so the scan never
//! reads it twice. A trailing `.`, `,`, `;`, `:`, `!`, `?` or closing bracket ends the sentence
//! around it and is not part of it.

use super::is_word;

/// The longest IPv6 address, `ffff:ffff:ffff:ffff:ffff:ffff:ffff:ffff`.
const IPV6_MAX: usize = 39;

/// `192.168.1.10`, every part up to 255, with `:8080` or not.
pub(super) fn ipv4(line: &[u8], at: usize) -> Option<usize> {
    if at > 0
        && line
            .get(at - 1)
            .is_some_and(|byte| is_word(*byte) || *byte == b'.')
    {
        return None;
    }
    let mut end = at;
    for part in 0..4 {
        if part > 0 {
            if line.get(end) != Some(&b'.') {
                return None;
            }
            end += 1;
        }
        let run = digit_run(line, end, 4);
        let value = line
            .get(end..end + run)
            .and_then(|digits| std::str::from_utf8(digits).ok())
            .and_then(|digits| digits.parse::<u16>().ok())?;
        if run > 3 || value > 255 {
            return None;
        }
        end += run;
    }
    // `1.2.3.4.5` is a version, not an address.
    if line.get(end) == Some(&b'.') && line.get(end + 1).is_some_and(u8::is_ascii_digit) {
        return None;
    }
    let port = digit_run(line, end + 1, 6);
    if line.get(end) == Some(&b':') && (1..=5).contains(&port) {
        end += 1 + port;
    }
    (!line.get(end).copied().is_some_and(is_word)).then_some(end)
}

/// `2001:db8::1`, `fe80::1`, `::1`: hexadecimal groups of at most four digits, eight of them or
/// fewer with one `::`.
pub(super) fn ipv6(line: &[u8], at: usize) -> Option<usize> {
    if at > 0
        && line
            .get(at - 1)
            .is_some_and(|byte| is_word(*byte) || matches!(byte, b':' | b'.'))
    {
        return None;
    }
    let rest = line.get(at..)?;
    let mut length = rest
        .iter()
        .take(IPV6_MAX + 1)
        .take_while(|byte| byte.is_ascii_hexdigit() || **byte == b':')
        .count();
    // A `:` after the address belongs to the sentence (`fe80::1: up`), `::` to the address.
    if length > 2 && rest.get(length - 1) == Some(&b':') && rest.get(length - 2) != Some(&b':') {
        length -= 1;
    }
    let candidate = rest.get(..length)?;
    if length > IPV6_MAX
        || rest
            .get(length)
            .copied()
            .is_some_and(|byte| is_word(byte) || byte == b'.')
    {
        return None;
    }
    let shortened = candidate.windows(2).filter(|pair| *pair == b"::").count();
    if shortened > 1 || candidate.windows(3).any(|three| three == b":::") {
        return None;
    }
    let groups = candidate.split(|byte| *byte == b':');
    let mut full = 0usize;
    for group in groups {
        if group.len() > 4 {
            return None;
        }
        full += usize::from(!group.is_empty());
    }
    let valid = if shortened == 1 {
        (1..=7).contains(&full)
    } else {
        full == 8 && !candidate.starts_with(b":") && !candidate.ends_with(b":")
    };
    valid.then_some(at + length)
}

/// `https://example.com/x?y=1`: a scheme, `://` and at least one more character.
pub(super) fn url(line: &[u8], at: usize) -> Option<usize> {
    if at > 0 && line.get(at - 1).copied().is_some_and(is_word) {
        return None;
    }
    let rest = line.get(at..)?;
    let scheme = rest
        .iter()
        .take(32)
        .take_while(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'.' | b'-'))
        .count();
    if scheme == 0 || !rest.get(scheme..)?.starts_with(b"://") {
        return None;
    }
    let end = run_end(line, at + scheme + 3);
    (end > at + scheme + 3).then_some(end)
}

/// A path at `at`: `C:\x`, `C:/x`, `\\server\share`, `/x`, `./x`, `../x` or `~/x`.
pub(super) fn path(line: &[u8], at: usize) -> Option<usize> {
    let before = at
        .checked_sub(1)
        .and_then(|before| line.get(before))
        .copied();
    if before.is_some_and(|byte| is_word(byte) || matches!(byte, b'/' | b'\\' | b'.' | b'-' | b'~'))
    {
        return None;
    }
    let rest = line.get(at..)?;
    let first = *rest.first()?;
    let start = if first.is_ascii_alphabetic() {
        // A drive: `C:\` or `C:/`.
        (rest.get(1) == Some(&b':') && matches!(rest.get(2), Some(b'\\' | b'/'))).then_some(3)?
    } else if rest.starts_with(b"\\\\") || rest.starts_with(b"./") || rest.starts_with(b"~/") {
        2
    } else if rest.starts_with(b"../") {
        3
    } else if first == b'/' {
        1
    } else {
        return None;
    };
    // At least one character of a name after the start: `/` alone or `//` is no path.
    if !rest
        .get(start)
        .copied()
        .is_some_and(|byte| is_word(byte) || matches!(byte, b'.' | b'~' | b'$' | b'%'))
    {
        return None;
    }
    Some(run_end(line, at + start))
}

/// Where a URL or path that goes on at `at` ends: at a space, a quote, `<`, `>`, `|` or the line's
/// end, without the punctuation before it.
fn run_end(line: &[u8], at: usize) -> usize {
    let rest = line.get(at..).unwrap_or_default();
    let mut end = at
        + rest
            .iter()
            .take_while(|byte| {
                !byte.is_ascii_whitespace()
                    && !matches!(byte, b'"' | b'\'' | b'<' | b'>' | b'|' | b'`')
            })
            .count();
    while end > at
        && line.get(end - 1).is_some_and(|byte| {
            matches!(
                byte,
                b'.' | b',' | b';' | b':' | b'!' | b'?' | b')' | b']' | b'}'
            )
        })
    {
        end -= 1;
    }
    end
}

/// How many digits stand at `at`, counting at most `most`.
fn digit_run(line: &[u8], at: usize, most: usize) -> usize {
    line.get(at..)
        .unwrap_or_default()
        .iter()
        .take(most)
        .take_while(|byte| byte.is_ascii_digit())
        .count()
}

#[cfg(test)]
#[path = "addresses_tests.rs"]
mod tests;
