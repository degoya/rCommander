//! Directory listings as [`Entry`] values: `MLSD`/`MLST` facts (RFC 3659) and the two `LIST`
//! formats servers print, Unix `ls -l` and DOS/IIS.
//!
//! Pure functions over one decoded line, tested against fixtures of real server answers
//! (`tests/fixtures/ftp_list/`). A line that is neither a fact list nor a recognised `LIST` line
//! is skipped rather than guessed at; `.`, `..` and `ls`'s `total` line name no entry.
//!
//! `LIST` times carry no zone and are taken as UTC; `Mon DD HH:MM` has no year, which is the
//! current one unless that would put the stamp in the future (then the year before, as `ls`
//! prints the time only for the last six months).

use std::{
    path::PathBuf,
    time::{Duration, SystemTime},
};

use rc_vfs::{Attributes, Entry, EntryKind};

use super::time::{Civil, month_number, parse_stamp};
pub(crate) use crate::listed_name::safe_name;

/// Which command produced a line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Dialect {
    /// `MLSD`: `fact=value;...; name`.
    Mlsd,
    /// `LIST`: whatever the server's `ls` or `dir` prints.
    List,
}

/// How far a `Mon DD HH:MM` stamp may lie ahead of `now` (zones, clock skew) before it is read
/// as last year's.
const FUTURE_SLACK: Duration = Duration::from_secs(2 * 86_400);

/// One line of a listing, or `None` for a line that names no entry - or names one that is not
/// a single name in this directory (`safe_name`).
pub(crate) fn parse_line(line: &str, dialect: Dialect, now: SystemTime) -> Option<Entry> {
    let line = line.trim_end_matches(['\r', '\n']);
    if line.trim().is_empty() {
        return None;
    }
    let entry = match dialect {
        Dialect::Mlsd if has_facts(line) => parse_facts(line),
        // Some servers answer MLSD with LIST lines; read them as such, never as a name.
        Dialect::Mlsd | Dialect::List => parse_list(line, now),
    }?;
    safe_name(&entry.name).then_some(entry)
}

/// A `LIST` line in Unix or DOS format.
pub(crate) fn parse_list(line: &str, now: SystemTime) -> Option<Entry> {
    if line.starts_with("total ") {
        return None;
    }
    parse_unix(line, now).or_else(|| parse_dos(line))
}

/// Whether a line carries RFC 3659 facts before its name.
fn has_facts(line: &str) -> bool {
    line.split_once("; ")
        .is_some_and(|(facts, name)| !name.is_empty() && facts.contains('='))
}

/// The entry of an `MLSD` line or an `MLST` answer line; the name is whatever follows the
/// facts (for `MLST` a path, whose last component the caller replaces).
pub(crate) fn parse_facts(line: &str) -> Option<Entry> {
    let line = line.trim_start();
    let (facts, name) = line.split_once("; ")?;
    let mut kind = EntryKind::File;
    let mut size = 0;
    let mut modified = None;
    let mut created = None;
    let mut mode = None;
    let mut perm: Option<&str> = None;
    for fact in facts.split(';') {
        let Some((key, value)) = fact.split_once('=') else {
            continue;
        };
        match key.to_ascii_lowercase().as_str() {
            "type" => match value.to_ascii_lowercase().as_str() {
                "file" => kind = EntryKind::File,
                "dir" => kind = EntryKind::Dir,
                "cdir" | "pdir" => return None,
                other => kind = link_or_other(value, other),
            },
            "size" => size = value.parse().ok()?,
            "modify" => modified = parse_stamp(value),
            "create" => created = parse_stamp(value),
            "unix.mode" => mode = u32::from_str_radix(value, 8).ok().map(|bits| bits & 0o7777),
            "perm" => perm = Some(value),
            _ => {}
        }
    }
    let attributes = match (mode, perm) {
        (Some(bits), _) => Attributes::from_mode(bits),
        (None, Some(perm)) => Attributes {
            readonly: !perm.contains(['w', 'a', 'c', 'm']),
            ..Attributes::default()
        },
        (None, None) => Attributes::default(),
    };
    named(name, kind, size, modified, created, attributes)
}

/// `OS.unix=slink:<target>` and its spellings; anything else unknown is `Other`.
fn link_or_other(value: &str, lower: &str) -> EntryKind {
    if !(lower.contains("slink") || lower.contains("symlink") || lower == "link") {
        return EntryKind::Other;
    }
    let target = value
        .split_once("slink:")
        .map(|(_, target)| PathBuf::from(target))
        .unwrap_or_default();
    EntryKind::Symlink {
        target,
        target_kind: None,
    }
}

/// The entry with the fields every dialect shares; `None` for `.`, `..` and an empty name.
fn named(
    name: &str,
    kind: EntryKind,
    size: u64,
    modified: Option<SystemTime>,
    created: Option<SystemTime>,
    attributes: Attributes,
) -> Option<Entry> {
    if name.is_empty() || name == "." || name == ".." {
        return None;
    }
    Some(Entry {
        name: name.to_owned(),
        size: if kind == EntryKind::Dir { 0 } else { size },
        kind,
        modified,
        created,
        attributes,
        hidden: name.starts_with('.'),
        owner: None,
    })
}

/// Whitespace-separated tokens with their byte offsets.
fn tokens(line: &str) -> Vec<(usize, &str)> {
    let mut out = Vec::new();
    let mut start = None;
    for (index, character) in line.char_indices() {
        match (character.is_whitespace(), start) {
            (true, Some(begin)) => {
                out.push((begin, &line[begin..index]));
                start = None;
            }
            (false, None) => start = Some(index),
            _ => {}
        }
    }
    if let Some(begin) = start {
        out.push((begin, &line[begin..]));
    }
    out
}

/// `drwxr-xr-x 2 owner group 4096 Jan 10 12:00 name`, with or without the group, a trailing
/// ACL marker on the permissions and `name -> target` for links.
pub(crate) fn parse_unix(line: &str, now: SystemTime) -> Option<Entry> {
    let tokens = tokens(line);
    let (_, permissions) = *tokens.first()?;
    let mode = unix_mode(permissions)?;
    let kind_char = permissions.chars().next()?;
    // The date is the first `Mon DD HH:MM|YYYY` after a numeric size; at least the link count
    // and the owner come before the size.
    let date_at = (3..tokens.len().saturating_sub(3)).find(|&index| {
        tokens[index - 1]
            .1
            .bytes()
            .all(|byte| byte.is_ascii_digit())
            && month_number(tokens[index].1).is_some()
            && tokens[index + 1]
                .1
                .parse::<u32>()
                .is_ok_and(|day| (1..=31).contains(&day))
            && (tokens[index + 2].1.contains(':') || tokens[index + 2].1.len() == 4)
    })?;
    let size: u64 = tokens[date_at - 1].1.parse().ok()?;
    let modified = unix_time(
        tokens[date_at].1,
        tokens[date_at + 1].1,
        tokens[date_at + 2].1,
        now,
    );
    let rest = &line[tokens[date_at + 3].0..];
    let (name, kind) = match kind_char {
        '-' => (rest, EntryKind::File),
        'd' => (rest, EntryKind::Dir),
        'l' => {
            let (name, target) = rest.split_once(" -> ").unwrap_or((rest, ""));
            let kind = EntryKind::Symlink {
                target: PathBuf::from(target),
                target_kind: None,
            };
            (name, kind)
        }
        _ => (rest, EntryKind::Other),
    };
    named(
        name,
        kind,
        size,
        modified,
        None,
        Attributes::from_mode(mode),
    )
}

/// The permission bits of `-rwsr-xr-t`-style text; `None` when it is not such text.
fn unix_mode(text: &str) -> Option<u32> {
    let bytes = text.as_bytes();
    if bytes.len() < 10 || !b"-dlbcps".contains(&bytes[0]) {
        return None;
    }
    // ACL (`+`), extended attributes (`@`) or SELinux context (`.`) may follow.
    if bytes[10..].iter().any(|byte| !b"+@.".contains(byte)) {
        return None;
    }
    let mut mode = 0;
    for (index, &byte) in bytes[1..10].iter().enumerate() {
        let bit = 1 << (8 - index);
        let special = match index {
            2 => 0o4000,
            5 => 0o2000,
            8 => 0o1000,
            _ => 0,
        };
        match (index % 3, byte) {
            (_, b'-') => {}
            (0, b'r') | (1, b'w') | (2, b'x') => mode |= bit,
            (2, b's' | b't') => mode |= bit | special,
            (2, b'S' | b'T') => mode |= special,
            _ => return None,
        }
    }
    Some(mode)
}

/// `Jan 10 12:00` (year inferred) or `Jan 10 2019` (midnight).
fn unix_time(month: &str, day: &str, time_or_year: &str, now: SystemTime) -> Option<SystemTime> {
    let month = month_number(month)?;
    let day = day.parse().ok()?;
    if let Some((hour, minute)) = time_or_year.split_once(':') {
        let this_year = Civil::from_system_time(now)?.year;
        let at = |year| {
            Civil {
                year,
                month,
                day,
                hour: hour.parse().ok()?,
                minute: minute.parse().ok()?,
                second: 0,
            }
            .to_system_time()
        };
        let stamp = at(this_year)?;
        if stamp > now + FUTURE_SLACK {
            return at(this_year - 1);
        }
        return Some(stamp);
    }
    Civil {
        year: time_or_year.parse().ok()?,
        month,
        day,
        hour: 0,
        minute: 0,
        second: 0,
    }
    .to_system_time()
}

/// `01-10-26  12:00PM  <DIR>  name` and `2026-01-10  12:00  1,234  name`, as IIS and other
/// Windows servers print them.
pub(crate) fn parse_dos(line: &str) -> Option<Entry> {
    let tokens = tokens(line);
    if tokens.len() < 4 {
        return None;
    }
    let date = dos_date(tokens[0].1)?;
    let (mut hour, minute, suffix) = dos_time(tokens[1].1)?;
    let mut next = 2;
    let meridiem = match suffix {
        "" if matches!(tokens[2].1, "AM" | "PM") => {
            next = 3;
            tokens[2].1
        }
        other => other,
    };
    match meridiem {
        "AM" if hour == 12 => hour = 0,
        "PM" if hour < 12 => hour += 12,
        "" | "AM" | "PM" => {}
        _ => return None,
    }
    let (_, size_or_dir) = *tokens.get(next)?;
    let name = &line[tokens.get(next + 1)?.0..];
    let (kind, size) = if size_or_dir.eq_ignore_ascii_case("<DIR>") {
        (EntryKind::Dir, 0)
    } else {
        (EntryKind::File, size_or_dir.replace(',', "").parse().ok()?)
    };
    let modified = Civil {
        year: date.0,
        month: date.1,
        day: date.2,
        hour,
        minute,
        second: 0,
    }
    .to_system_time();
    named(name, kind, size, modified, None, Attributes::default())
}

/// `MM-DD-YY`, `MM-DD-YYYY` or `YYYY-MM-DD`; two-digit years below 70 are this century.
fn dos_date(text: &str) -> Option<(i64, u32, u32)> {
    let parts: Vec<&str> = text.split('-').collect();
    let [first, second, third] = parts.as_slice() else {
        return None;
    };
    if !parts
        .iter()
        .all(|part| part.bytes().all(|byte| byte.is_ascii_digit()))
    {
        return None;
    }
    if first.len() == 4 {
        return Some((
            first.parse().ok()?,
            second.parse().ok()?,
            third.parse().ok()?,
        ));
    }
    let year: i64 = third.parse().ok()?;
    let year = match third.len() {
        2 if year < 70 => 2000 + year,
        2 => 1900 + year,
        4 => year,
        _ => return None,
    };
    Some((year, first.parse().ok()?, second.parse().ok()?))
}

/// `HH:MM` with an optional `AM`/`PM` glued on.
fn dos_time(text: &str) -> Option<(u32, u32, &str)> {
    let (hour, rest) = text.split_once(':')?;
    let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
    let (minute, suffix) = rest.split_at(digits);
    Some((hour.parse().ok()?, minute.parse().ok()?, suffix))
}

#[cfg(test)]
#[path = "listing_tests.rs"]
mod tests;
