//! A part of an archive name made a name Windows can hold, as 7-Zip unpacks it on Windows
//! (RC-110-92): a character Windows refuses becomes `_`, a device name gets a `_` in front, a dot
//! or space at the end becomes `_`. The result passes the one name rule of `rc-rename`
//! (`check_plain`) but for a character that turns the reading direction or a length over 255
//! bytes, which unpacking still skips.
//!
//! Without it a part `C:` joined to the target is a drive (`target\a`.join(`C:`) is `C:`), and
//! `x.exe:Zone.Identifier` is a data stream of `x.exe`.

/// The characters Windows refuses in a name besides the separators and control characters.
const RESERVED_CHARS: &[char] = &['<', '>', ':', '"', '|', '?', '*'];

/// The device names Windows reserves, with any extension; `COM` and `LPT` with a digit are
/// [`is_device`]'s own case.
const DEVICES: &[&str] = &["CON", "PRN", "AUX", "NUL", "CONIN$", "CONOUT$"];

/// `part` - one part of a cleaned name, never empty, `.`, `..` or holding a separator - as a
/// name Windows can hold; `None` when it is one already.
pub(super) fn holdable(part: &str) -> Option<String> {
    let refused = |c: char| c.is_control() || RESERVED_CHARS.contains(&c);
    let mut name: String = part
        .chars()
        .map(|c| if refused(c) { '_' } else { c })
        .collect();
    // `.` and ` ` are one byte each: the bytes cut are the characters to put back as `_`.
    let kept = name.trim_end_matches(['.', ' ']).len();
    let trailing = name.len() - kept;
    name.truncate(kept);
    name.extend(std::iter::repeat_n('_', trailing));
    if is_device(&name) {
        name.insert(0, '_');
    }
    (name != part).then_some(name)
}

/// `CON`, `nul.txt`, `COM1.tar.gz`, `lpt\u{b9}`: the part before the first dot, without trailing
/// spaces, is a device name - the rule `rc-rename` holds every name to.
fn is_device(name: &str) -> bool {
    let stem = name.split('.').next().unwrap_or(name).trim_end_matches(' ');
    let upper = stem.to_uppercase();
    if DEVICES.contains(&upper.as_str()) {
        return true;
    }
    let Some(number) = upper
        .strip_prefix("COM")
        .or_else(|| upper.strip_prefix("LPT"))
    else {
        return false;
    };
    let mut digits = number.chars();
    matches!(
        (digits.next(), digits.next()),
        (Some('0'..='9' | '\u{b9}' | '\u{b2}' | '\u{b3}'), None)
    )
}

#[cfg(test)]
#[path = "windows_name_tests.rs"]
mod tests;
