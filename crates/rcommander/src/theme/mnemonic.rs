//! Access keys of a dialog (RC-080-16), as Windows and Total Commander have them: a button's
//! label in `locales/en.json` marks one letter with `&` (`"&Overwrite"`, `"S&kip all"`; `&&` is
//! a literal ampersand), the button draws its label without the mark and with the letter
//! underlined, and Alt with the letter presses it - the letter alone too while a button or a
//! checkbox of the dialog has the focus ring, never while a text field, a list or a pick list
//! has the keyboard (they take letters themselves). [`Scope`] round each window's content
//! presses them in the top dialog card, as Tab walks that card's ring (`app/field_walk.rs`);
//! `theme::focus::Stop` carries the letter.
//!
//! The underline shows while Alt is held, as in Windows, or always when the setting
//! `keys.underline_access_keys` says so ([`set_always`]).

mod scope;

use std::{
    borrow::Cow,
    sync::atomic::{AtomicBool, Ordering},
};

use iced::{
    event::Event,
    keyboard::{self, Modifiers},
};

pub use scope::Scope;

/// The setting `keys.underline_access_keys`, read at the start and on every change.
static ALWAYS: AtomicBool = AtomicBool::new(false);
/// Whether Alt is held, from the window's modifier changes.
static ALT_HELD: AtomicBool = AtomicBool::new(false);

/// Whether the access keys are underlined always, or only while Alt is held.
pub fn set_always(on: bool) {
    ALWAYS.store(on, Ordering::Relaxed);
}

/// The modifiers changed: the underline follows Alt.
pub fn set_modifiers(modifiers: Modifiers) {
    ALT_HELD.store(modifiers.alt(), Ordering::Relaxed);
}

/// Whether a label draws its access key underlined now.
pub fn shown() -> bool {
    shown_when(
        ALWAYS.load(Ordering::Relaxed),
        ALT_HELD.load(Ordering::Relaxed),
    )
}

fn shown_when(always: bool, alt: bool) -> bool {
    always || alt
}

/// A label with its mark read: the text to draw and the access key, if it has one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Label<'a> {
    /// The label without the mark; `&&` as one `&`.
    pub text: Cow<'a, str>,
    /// The access key in lower case.
    pub letter: Option<char>,
    /// Where the access key is in `text`, in bytes.
    at: Option<usize>,
}

impl<'a> Label<'a> {
    /// `label` as the catalogue writes it: the first `&` before a letter or a digit marks it (a
    /// later one is dropped); `&&` is an ampersand, and so is an `&` before anything else.
    pub fn parse(label: &'a str) -> Self {
        if !label.contains('&') {
            return Self {
                text: Cow::Borrowed(label),
                letter: None,
                at: None,
            };
        }
        let mut text = String::with_capacity(label.len());
        let (mut letter, mut at) = (None, None);
        let mut chars = label.chars();
        while let Some(c) = chars.next() {
            if c != '&' {
                text.push(c);
                continue;
            }
            match chars.next() {
                Some('&') => text.push('&'),
                Some(next) if next.is_alphanumeric() => {
                    if letter.is_none() {
                        at = Some(text.len());
                        letter = next.to_lowercase().next();
                    }
                    text.push(next);
                }
                Some(next) => {
                    text.push('&');
                    text.push(next);
                }
                None => text.push('&'),
            }
        }
        Self {
            text: Cow::Owned(text),
            letter,
            at,
        }
    }

    /// The text before the access key, the key as written, and the text after it; `None` for a
    /// label without one.
    pub fn split(&self) -> Option<(&str, &str, &str)> {
        let at = self.at?;
        let (before, rest) = self.text.split_at(at);
        let width = rest.chars().next()?.len_utf8();
        let (key, after) = rest.split_at(width);
        Some((before, key, after))
    }
}

/// The access key `event` presses, and whether Alt is held: a letter or a digit without Ctrl or
/// the Windows key (AltGr is Ctrl+Alt: it types), in lower case; on a layout without Latin
/// letters the key's place on a Latin keyboard, as Windows maps it.
pub fn pressed(event: &Event) -> Option<(char, bool)> {
    let Event::Keyboard(keyboard::Event::KeyPressed {
        key,
        physical_key,
        modifiers,
        ..
    }) = event
    else {
        return None;
    };
    if modifiers.control() || modifiers.logo() {
        return None;
    }
    let letter = key.to_latin(*physical_key)?.to_lowercase().next()?;
    letter
        .is_alphanumeric()
        .then_some((letter, modifiers.alt()))
}

#[cfg(test)]
#[path = "mnemonic_tests.rs"]
mod tests;
