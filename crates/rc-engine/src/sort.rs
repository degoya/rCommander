//! The order of a panel's entries: directories first, then by a key - name, extension, size or
//! date - with names compared naturally (`file2` before `file10`) and without regard to case,
//! as in Total Commander. Names follow Windows' word sort, as Total Commander shows them:
//! punctuation and symbols (`_`, `$`, `[`, ...) before digits, digits before letters, and the
//! hyphen and the apostrophe count only between names that are otherwise equal - so
//! `--- incoming ---` sorts as ` incoming ` would, before `$RECYCLE.BIN` (owner, 2026-10-19).
//!
//! `sort.names` chooses another order (owner, 2026-10-19): the byte order of Linux' `ls` in the C
//! locale (case counts, symbols by their code), or each by the panel's target - Windows' on the
//! drives of this Windows machine, Linux' on SSH, FTP and inside WSL ([`names_for`]).
//! `sort.numbers_by_value` switches the natural order of digits off for both. The engine holds
//! both for the process ([`set_names`]), as it reads them from the settings.

use std::{
    cmp::Ordering,
    sync::atomic::{AtomicBool, AtomicU8, Ordering as Atomic},
};

use rc_vfs::{BackendId, Entry, EntryKind, VfsPath, parse_wsl_unc, split_name};
use serde::{Deserialize, Serialize};

/// What a panel sorts by.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SortKey {
    #[default]
    Name,
    Ext,
    Size,
    Date,
}

impl SortKey {
    /// Whether the key starts out descending: the largest and the newest first, as in Total
    /// Commander; names and extensions start from A.
    #[must_use]
    pub fn descending_first(self) -> bool {
        matches!(self, Self::Size | Self::Date)
    }
}

/// A key and its direction.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SortOrder {
    pub key: SortKey,
    pub descending: bool,
}

impl SortOrder {
    /// The order after choosing `key`, as a click on a column header or Ctrl+F3 to F6 does: the
    /// same key again turns the direction around, another key starts in its first direction.
    #[must_use]
    pub fn choose(self, key: SortKey) -> Self {
        let descending = if key == self.key {
            !self.descending
        } else {
            key.descending_first()
        };
        Self { key, descending }
    }
}

/// Which order names follow (`sort.names`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum NameOrder {
    /// Windows' word sort, as Total Commander shows it.
    #[default]
    Windows,
    /// The byte order of `ls` in the C locale.
    Linux,
    /// Windows' on this Windows machine's drives, Linux' on servers and inside WSL.
    ByTarget,
}

impl NameOrder {
    /// The stored value: `windows`, `linux` or `by_target`; anything else is Windows'.
    #[must_use]
    pub fn from_setting(value: &str) -> Self {
        match value {
            "linux" => Self::Linux,
            "by_target" => Self::ByTarget,
            _ => Self::Windows,
        }
    }
}

/// How names compare in one panel: [`NameOrder`] resolved for its target.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Names {
    /// Linux' byte order, else Windows' word sort.
    pub linux: bool,
    /// Runs of digits by their value.
    pub numbers: bool,
}

impl Names {
    /// Windows' word sort with numbers by value: the default, Total Commander's.
    pub const WINDOWS: Self = Self {
        linux: false,
        numbers: true,
    };
}

static ORDER: AtomicU8 = AtomicU8::new(0);
static NUMBERS: AtomicBool = AtomicBool::new(true);

/// The order names follow from now on, for the whole process.
pub fn set_names(order: NameOrder, numbers: bool) {
    let order = match order {
        NameOrder::Windows => 0,
        NameOrder::Linux => 1,
        NameOrder::ByTarget => 2,
    };
    ORDER.store(order, Atomic::Relaxed);
    NUMBERS.store(numbers, Atomic::Relaxed);
}

/// How names compare in a panel showing `path`.
#[must_use]
pub fn names_for(path: &VfsPath) -> Names {
    let linux = match ORDER.load(Atomic::Relaxed) {
        0 => false,
        1 => true,
        _ => linux_target(path),
    };
    Names {
        linux,
        numbers: NUMBERS.load(Atomic::Relaxed),
    }
}

/// Whether `path` lies on a Linux system: a server, a WSL distribution, or this machine when it
/// is no Windows machine; an archive goes by where its file lies.
fn linux_target(path: &VfsPath) -> bool {
    match &path.backend {
        BackendId::Sftp { .. } | BackendId::Ftp { .. } => true,
        BackendId::Archive { outer, .. } => linux_target(outer),
        _ => !cfg!(windows) || parse_wsl_unc(&path.path).is_some(),
    }
}

/// What follows the last dot of a file's name - never of a directory, and not the leading dot of
/// a hidden file such as `.bashrc`. In a flat view the name carries its folders
/// (`sub.d\file`); only the last part counts.
#[must_use]
pub fn extension(entry: &Entry) -> &str {
    if entry.is_dir_like() || entry.kind == EntryKind::Other {
        return "";
    }
    let start = entry
        .name
        .rfind(std::path::is_separator)
        .map_or(0, |separator| separator + 1);
    split_name(&entry.name[start..]).1
}

/// `a` against `b` ignoring case, runs of digits compared by their value: `file2` < `File10`;
/// hyphens and apostrophes decide only between names equal without them, as in Windows.
#[must_use]
pub fn natural_cmp(a: &str, b: &str) -> Ordering {
    compare_names(a, b, Names::WINDOWS)
}

/// `a` against `b` in the order `names` says: Windows' word sort ignoring case, or Linux' byte
/// order; runs of digits by their value with `names.numbers`.
#[must_use]
pub fn compare_names(a: &str, b: &str, names: Names) -> Ordering {
    match (names.linux, names.numbers) {
        (true, false) => a.cmp(b),
        (true, true) => byte_cmp(a, b),
        (false, numbers) => {
            word_cmp(a, b, true, numbers).then_with(|| word_cmp(a, b, false, numbers))
        }
    }
}

/// `a` against `b` by their characters' codes - UTF-8's byte order -, runs of digits by value.
fn byte_cmp(a: &str, b: &str) -> Ordering {
    let (mut a, mut b) = (a, b);
    loop {
        let (Some(x), Some(y)) = (a.chars().next(), b.chars().next()) else {
            return a.is_empty().cmp(&b.is_empty()).reverse();
        };
        if x.is_ascii_digit() && y.is_ascii_digit() {
            let (order, x_rest, y_rest) = digits_cmp(a, b);
            if order != Ordering::Equal {
                return order;
            }
            (a, b) = (x_rest, y_rest);
        } else {
            let order = x.cmp(&y);
            if order != Ordering::Equal {
                return order;
            }
            (a, b) = (&a[x.len_utf8()..], &b[y.len_utf8()..]);
        }
    }
}

/// The leading runs of digits of `a` and `b` by their value, and what follows each.
fn digits_cmp<'a, 'b>(a: &'a str, b: &'b str) -> (Ordering, &'a str, &'b str) {
    let (x_digits, x_rest) = split_digits(a);
    let (y_digits, y_rest) = split_digits(b);
    let x_value = x_digits.trim_start_matches('0');
    let y_value = y_digits.trim_start_matches('0');
    let order = x_value
        .len()
        .cmp(&y_value.len())
        .then_with(|| x_value.cmp(y_value));
    (order, x_rest, y_rest)
}

/// The characters Windows' word sort passes over: they decide only a tie.
fn ignored(c: char) -> bool {
    matches!(c, '-' | '\'')
}

/// `a` against `b` in Windows' word sort, passing over [`ignored`] characters when `skip`, runs
/// of digits by their value with `numbers`.
fn word_cmp(a: &str, b: &str, skip: bool, numbers: bool) -> Ordering {
    let (mut a, mut b) = (a, b);
    loop {
        if skip {
            (a, b) = (a.trim_start_matches(ignored), b.trim_start_matches(ignored));
        }
        let (Some(x), Some(y)) = (a.chars().next(), b.chars().next()) else {
            // The shorter name first; equal when both ended.
            return a.is_empty().cmp(&b.is_empty()).reverse();
        };
        if numbers && x.is_ascii_digit() && y.is_ascii_digit() {
            let (order, x_rest, y_rest) = digits_cmp(a, b);
            if order != Ordering::Equal {
                return order;
            }
            (a, b) = (x_rest, y_rest);
        } else {
            let order = class(x)
                .cmp(&class(y))
                .then_with(|| symbol_rank(x).cmp(&symbol_rank(y)))
                .then_with(|| x.to_lowercase().cmp(y.to_lowercase()));
            if order != Ordering::Equal {
                return order;
            }
            (a, b) = (&a[x.len_utf8()..], &b[y.len_utf8()..]);
        }
    }
}

/// Where a character stands in Windows' order: symbols and punctuation, then digits, then
/// letters (any script); a hyphen or an apostrophe, deciding a tie, after all of them.
fn class(c: char) -> u8 {
    if ignored(c) {
        3
    } else if c.is_ascii_digit() {
        1
    } else if c.is_alphanumeric() {
        2
    } else {
        0
    }
}

/// The ASCII symbols in the order Windows sorts them; other symbols follow by code point.
const SYMBOLS: &str = " !\"#$%&()*,./:;?@[\\]^_`{|}~+<=>";

/// A symbol's place among the symbols; letters and digits all rank alike here.
fn symbol_rank(c: char) -> u32 {
    if class(c) != 0 {
        return 0;
    }
    SYMBOLS.find(c).map_or(u32::from(c) + 128, |at| {
        u32::try_from(at).unwrap_or(u32::MAX)
    })
}

/// The leading ASCII digits of `text` and the rest.
fn split_digits(text: &str) -> (&str, &str) {
    let end = text
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(text.len());
    text.split_at(end)
}

/// Sorts `entries` in panel order: directories (and links to them) first, then by `order`, names
/// as `names` compares them. Directories have no extension and no size of their own, so by those
/// keys they stay in name order; equal keys fall back to the name, so the order is total.
pub fn sort_entries(entries: &mut [Entry], order: SortOrder, names: Names) {
    entries.sort_unstable_by(|a, b| {
        let (a_dir, b_dir) = (a.is_dir_like(), b.is_dir_like());
        b_dir.cmp(&a_dir).then_with(|| {
            let by_name = matches!(order.key, SortKey::Ext | SortKey::Size) && a_dir && b_dir;
            if by_name {
                by_names(a, b, names)
            } else if order.descending {
                compare(a, b, order.key, names).reverse()
            } else {
                compare(a, b, order.key, names)
            }
        })
    });
}

fn compare(a: &Entry, b: &Entry, key: SortKey, names: Names) -> Ordering {
    let by_key = match key {
        SortKey::Name => Ordering::Equal,
        SortKey::Ext => compare_names(extension(a), extension(b), names),
        SortKey::Size => a.size.cmp(&b.size),
        SortKey::Date => a.modified.cmp(&b.modified),
    };
    by_key.then_with(|| by_names(a, b, names))
}

fn by_names(a: &Entry, b: &Entry, names: Names) -> Ordering {
    compare_names(&a.name, &b.name, names).then_with(|| a.name.cmp(&b.name))
}

#[cfg(test)]
#[path = "sort_tests.rs"]
mod tests;
