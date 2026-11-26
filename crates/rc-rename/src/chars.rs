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
/// zero-width one (U+200B to U+200D, the byte-order mark U+FEFF), one that [`turns_direction`]
/// or another format character (Unicode's category Cf, RC-110-62).
#[must_use]
pub fn invisible(c: char) -> bool {
    c.is_control()
        || turns_direction(c)
        || matches!(c, '\u{200b}'..='\u{200d}' | '\u{feff}')
        || formats(c)
}

/// The format characters (Cf, Unicode 15.1) neither [`turns_direction`] nor the zero-width ones
/// cover: the soft hyphen, the prepended concatenation marks, the Mongolian vowel separator, the
/// word joiner and the invisible operators, the deprecated shaping controls, the interlinear
/// annotation marks, the format controls of Egyptian hieroglyphs, shorthand and musical notation,
/// and the tag characters (text hidden in a name, "ASCII smuggling").
fn formats(c: char) -> bool {
    matches!(
        c,
        '\u{ad}'
            | '\u{600}'..='\u{605}'
            | '\u{6dd}'
            | '\u{70f}'
            | '\u{890}'..='\u{891}'
            | '\u{8e2}'
            | '\u{180e}'
            | '\u{2060}'..='\u{2064}'
            | '\u{206a}'..='\u{206f}'
            | '\u{fff9}'..='\u{fffb}'
            | '\u{110bd}'
            | '\u{110cd}'
            | '\u{13430}'..='\u{1343f}'
            | '\u{1bca0}'..='\u{1bca3}'
            | '\u{1d173}'..='\u{1d17a}'
            | '\u{e0001}'
            | '\u{e0020}'..='\u{e007f}'
    )
}

#[cfg(test)]
#[path = "chars_tests.rs"]
mod tests;
