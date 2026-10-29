//! Lister's keys, as Total Commander has them, and a few of rCommander's own where Lister has
//! none (`E` for the next encoding, `T` for the tab width, `Ctrl+G` for "go to offset"). `5`
//! is Lister's formatted view of an HTML file; here it shows a Markdown file formatted. Ctrl++,
//! Ctrl+- and Ctrl+0 size the text, or zoom the picture in the image view (RC-020-12). `7` steps
//! through a plugin document's views (RC-100-10): Lister picks its modes by digit, and 7 is the
//! one digit of that row still free here.
//!
//! The keys are the viewer's while it is open, not the keymap's: they would collide with the
//! panels' bindings (N, P, digits), and a viewer has nothing an agent could call.

use rc_engine::{
    Chord,
    viewer::{TextEncoding, ViewMode},
};

/// What a key does in the viewer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListerKey {
    /// Escape: the viewer closes (a search running or a bar open goes first).
    Close,
    /// 1 to 4: text, binary, hex, image.
    Mode(ViewMode),
    /// 5: a Markdown file's preview on or off (RC-020-09).
    Preview,
    /// 7: a plugin document's next view - coloured text, tree, ... (RC-100-10).
    NextView,
    /// A: Windows-1252 (ANSI); S: CP437 (ASCII/DOS); 6: UTF-16 LE; 8: UTF-8 - Lister's mode
    /// keys for them.
    Encoding(TextEncoding),
    /// E: the next of the six encodings.
    NextEncoding,
    /// W: line wrap on or off.
    Wrap,
    /// T: the tab width 8, 4 or 2.
    TabWidth,
    Scroll(Scroll),
    /// N and P: the next or previous file of the panel.
    NextFile,
    PreviousFile,
    /// F2: the file read again.
    Reload,
    /// F7 or Ctrl+F: the search bar.
    Search,
    /// F3 and Shift+F3: the next match on, or back.
    FindNext,
    FindPrevious,
    /// Ctrl+G: the "go to offset" bar.
    GoTo,
    /// NumAdd, `+` or `=`, NumSubtract or `-`: the image larger or smaller.
    ZoomIn,
    ZoomOut,
    /// Ctrl++ (also Ctrl+=, Ctrl+Shift+= and Ctrl+NumAdd), Ctrl+- (Ctrl+NumSubtract) and Ctrl+0
    /// (Ctrl+Num0): the text a step larger, smaller or back to the size of the settings; in the
    /// image view the picture zooms as with `+` and `-`, and Ctrl+0 fits it (RC-020-12).
    TextLarger,
    TextSmaller,
    TextReset,
    /// F: the image fit to the window, or at 100 %.
    Fit,
    /// R and Shift+R: the image turned a quarter right or left (not saved).
    RotateRight,
    RotateLeft,
}

impl ListerKey {
    /// Whether the key sizes the text (RC-020-12).
    pub fn sizes_text(self) -> bool {
        matches!(self, Self::TextLarger | Self::TextSmaller | Self::TextReset)
    }
}

/// A scrolling key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scroll {
    LineUp,
    LineDown,
    PageUp,
    PageDown,
    Start,
    End,
    /// Without wrap: the text view's columns.
    Left,
    Right,
}

/// The viewer's action for `chord`, if it has one.
pub fn action(chord: &Chord) -> Option<ListerKey> {
    use ListerKey as K;
    let plain = chord.is_plain();
    let ctrl = chord.ctrl && !chord.alt && !chord.shift;
    let shift = chord.shift && !chord.ctrl && !chord.alt;
    Some(match chord.key.as_str() {
        "Escape" if plain => K::Close,
        "1" if plain => K::Mode(ViewMode::Text),
        "2" if plain => K::Mode(ViewMode::Binary),
        "3" if plain => K::Mode(ViewMode::Hex),
        "4" if plain => K::Mode(ViewMode::Image),
        "5" if plain => K::Preview,
        "7" if plain => K::NextView,
        "a" if plain => K::Encoding(TextEncoding::Windows1252),
        "s" if plain => K::Encoding(TextEncoding::Cp437),
        "6" if plain => K::Encoding(TextEncoding::Utf16Le),
        "8" if plain => K::Encoding(TextEncoding::Utf8),
        "e" if plain => K::NextEncoding,
        "w" if plain => K::Wrap,
        "t" if plain => K::TabWidth,
        "n" if plain => K::NextFile,
        "p" if plain => K::PreviousFile,
        "F2" if plain => K::Reload,
        "F7" if plain => K::Search,
        "f" if ctrl => K::Search,
        "F3" if plain => K::FindNext,
        "F3" if shift => K::FindPrevious,
        "g" if ctrl => K::GoTo,
        "ArrowUp" if plain => K::Scroll(Scroll::LineUp),
        "ArrowDown" if plain => K::Scroll(Scroll::LineDown),
        "ArrowLeft" if plain => K::Scroll(Scroll::Left),
        "ArrowRight" if plain => K::Scroll(Scroll::Right),
        "PageUp" if plain => K::Scroll(Scroll::PageUp),
        "PageDown" | "Space" if plain => K::Scroll(Scroll::PageDown),
        "Home" if plain || ctrl => K::Scroll(Scroll::Start),
        "End" if plain || ctrl => K::Scroll(Scroll::End),
        "NumAdd" | "Plus" | "=" if plain => K::ZoomIn,
        "NumSubtract" | "-" if plain => K::ZoomOut,
        // US layouts type `+` as Shift+=: Ctrl+Shift+= is Ctrl++ there.
        "=" if chord.ctrl && !chord.alt => K::TextLarger,
        "NumAdd" | "Plus" if ctrl => K::TextLarger,
        "NumSubtract" | "-" if ctrl => K::TextSmaller,
        "0" if ctrl => K::TextReset,
        "f" if plain => K::Fit,
        "r" if plain => K::RotateRight,
        "r" if shift => K::RotateLeft,
        _ => return None,
    })
}

#[cfg(test)]
#[path = "keys_tests.rs"]
mod tests;
