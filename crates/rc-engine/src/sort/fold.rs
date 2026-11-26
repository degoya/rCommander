//! A name as one pass of Windows' word sort reads it (RC-110-84). A letter with accents is read
//! as its canonical decomposition (NFD): its base letter, then its accents. The first pass passes
//! over the accents and reads the sharp s as `ss`, so an A with diaeresis sorts beside the A and
//! an E with acute among the E, as in Explorer and Total Commander; the second pass sees the
//! accents, after every other character, so a name without them comes first. A composed and a
//! decomposed name (as macOS writes them) read alike in both passes. Only accents fold: a letter
//! whose decomposition holds anything else - a Hangul syllable, an Indic vowel sign - stays as
//! it is. Nothing is allocated per comparison; an ASCII character costs no table lookup.

use unicode_normalization::char::decompose_canonical;

/// The pending characters a cursor holds at most: the second `s` of a sharp s, or the accents of
/// one letter (canonical decompositions carry up to three); more are dropped.
const PENDING: usize = 3;

/// Whether `c` is an accent: a combining diacritic of Latin, Greek and Cyrillic letters, or a
/// kana voicing mark.
pub(super) fn mark(c: char) -> bool {
    matches!(
        c,
        '\u{300}'..='\u{36F}'
            | '\u{1AB0}'..='\u{1AFF}'
            | '\u{1DC0}'..='\u{1DFF}'
            | '\u{20D0}'..='\u{20FF}'
            | '\u{FE20}'..='\u{FE2F}'
            | '\u{3099}'
            | '\u{309A}'
    )
}

/// The sharp s and its capital, read as `ss` in the first pass.
fn sharp_s(c: char) -> bool {
    matches!(c, '\u{DF}' | '\u{1E9E}')
}

/// How a cursor reads a name: the passes of Windows' word sort, each deciding only where the one
/// before found a tie. Hyphens and apostrophes are passed over in the first two.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Read {
    /// Base letters only: accents passed over, the sharp s as `ss` - `Apfel`, `Apfelbaum` and
    /// `Apfel` with an umlaut start alike.
    Base,
    /// Base letters, each followed by its accents, which sort after every other character:
    /// `Apfel` before its umlaut.
    Accents,
    /// The characters as they are; case still does not count.
    Raw,
}

/// What is left of a name in one pass of the comparison.
#[derive(Clone, Copy)]
pub(super) struct Cursor<'a> {
    /// The characters not read yet.
    pub(super) rest: &'a str,
    read: Read,
    /// Characters of the last one read still to come, the next at `pending[next]`.
    pending: [char; PENDING],
    next: usize,
    len: usize,
}

impl<'a> Cursor<'a> {
    pub(super) fn new(name: &'a str, read: Read) -> Self {
        Self {
            rest: name,
            read,
            pending: ['\0'; PENDING],
            next: 0,
            len: 0,
        }
    }

    fn has_pending(&self) -> bool {
        self.next < self.len
    }

    /// Passes over the leading characters `skip` names.
    pub(super) fn skip(&mut self, skip: impl Fn(char) -> bool) {
        if !self.has_pending() {
            self.rest = self.rest.trim_start_matches(skip);
        }
    }

    /// Whether an ASCII digit comes next.
    pub(super) fn at_digit(&self) -> bool {
        !self.has_pending() && self.rest.as_bytes().first().is_some_and(u8::is_ascii_digit)
    }

    /// The next character as this pass reads it; `None` at the end.
    pub(super) fn next_char(&mut self) -> Option<char> {
        if self.has_pending() {
            self.next += 1;
            return self.pending.get(self.next - 1).copied();
        }
        let c = self.rest.chars().next()?;
        self.rest = &self.rest[c.len_utf8()..];
        if self.read == Read::Raw || c.is_ascii() {
            return Some(c);
        }
        if self.read == Read::Base && sharp_s(c) {
            self.hold(&['s']);
            return Some('s');
        }
        Some(self.decompose(c))
    }

    /// The base letter of `c`, its accents held for [`Read::Accents`]; `c` itself when it has
    /// no decomposition or one that is not a letter and its accents.
    fn decompose(&mut self, c: char) -> char {
        let mut parts = ['\0'; PENDING + 1];
        let (mut count, mut accents_only) = (0, true);
        decompose_canonical(c, |part| {
            if count > 0 {
                accents_only &= mark(part);
            }
            if let Some(slot) = parts.get_mut(count) {
                *slot = part;
            }
            count += 1;
        });
        if count < 2 || !accents_only {
            return c;
        }
        if self.read == Read::Accents {
            let end = count.min(parts.len());
            self.hold(parts.get(1..end).unwrap_or_default());
        }
        parts[0]
    }

    /// Holds `chars` to be read before the rest.
    fn hold(&mut self, chars: &[char]) {
        let len = chars.len().min(PENDING);
        self.pending[..len].copy_from_slice(&chars[..len]);
        (self.next, self.len) = (0, len);
    }
}
