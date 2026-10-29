//! A letter typed in a menu or a popup, as a Win32 menu without underlined letters takes it
//! (RC-080-16): the entries whose label starts with the letter, ignoring case and a leading mark
//! (the `*` of a locked tab); one alone is taken at once, of several the cursor goes to the next
//! after it, wrapping around. The file list's context menu, a drive's context menu and the
//! popups of the tabs use it; the drive menu has its own (a drive letter takes its drive).

/// What a letter does in a menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Letter {
    /// No entry starts with it.
    None,
    /// Several do: the cursor goes to this one.
    Move(usize),
    /// Only this one does: it is taken.
    Take(usize),
}

/// What `key` (one character) does in a menu of `labels` - `None` for a row the cursor skips,
/// a heading - with the cursor on `cursor`.
pub fn letter<'a>(
    key: &str,
    labels: impl IntoIterator<Item = Option<&'a str>>,
    cursor: usize,
) -> Letter {
    let key = key.to_lowercase();
    if key.is_empty() {
        return Letter::None;
    }
    let hits: Vec<usize> = labels
        .into_iter()
        .enumerate()
        .filter(|(_, label)| {
            label.is_some_and(|label| {
                label
                    .trim_start_matches(|c: char| !c.is_alphanumeric())
                    .to_lowercase()
                    .starts_with(&key)
            })
        })
        .map(|(index, _)| index)
        .collect();
    match hits.as_slice() {
        [] => Letter::None,
        [only] => Letter::Take(*only),
        many => Letter::Move(
            many.iter()
                .copied()
                .find(|&index| index > cursor)
                .unwrap_or(many[0]),
        ),
    }
}

/// Whether `chord` is a letter for [`letter`]: one character, without Ctrl or Alt.
pub fn is_letter(chord: &rc_engine::Chord) -> bool {
    !chord.ctrl && !chord.alt && chord.key.chars().count() == 1
}

#[cfg(test)]
#[path = "menu_letter_tests.rs"]
mod tests;
