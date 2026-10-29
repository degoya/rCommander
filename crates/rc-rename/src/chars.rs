//! The characters a person cannot see where text shows them - the one list rCommander keeps
//! (RC-080-43). A new name may not hold one that turns the reading direction
//! ([`turns_direction`]); text from outside (an MCP call, a plugin's message) shows every
//! [`invisible`] one spelled out.

/// Whether `c` changes the direction text is read in - the marks, embeddings, overrides and
/// isolates: `invoice<U+202E>fdp.exe` reads as `invoiceexe.pdf` (RC-080-12).
#[must_use]
pub fn turns_direction(c: char) -> bool {
    matches!(
        c,
        '\u{61c}' | '\u{200e}' | '\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}'
    )
}

/// Whether `c` shows as nothing or changes how the text around it shows: a control character, a
/// zero-width one (U+200B to U+200D, the byte-order mark U+FEFF) or one that
/// [`turns_direction`].
#[must_use]
pub fn invisible(c: char) -> bool {
    c.is_control() || turns_direction(c) || matches!(c, '\u{200b}'..='\u{200d}' | '\u{feff}')
}

#[cfg(test)]
#[path = "chars_tests.rs"]
mod tests;
