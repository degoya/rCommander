//! The person's deviations from the standard keymap (RC-100-21), as the setting
//! `keys.overrides` keeps them: one word per deviation, separated by spaces - `Ctrl+K=copy`, the
//! chord now runs the standard row named `copy`, and `F5=`, the chord runs nothing any more. A
//! chord bound to another row replaces what ran on it. Only the deviations are kept, so new
//! standard keys of a later version still arrive; a word naming a row that version no longer
//! has is left out when the keymap is built ([`Keymap::with_overrides`]) and dropped at the
//! next save.
//!
//! The text is read as untrusted input - it comes back with an import (`config.import`): at
//! most [`MAX_OVERRIDES`] words and [`MAX_OVERRIDE_TEXT`] characters, every chord one a chord
//! can name and none the window keeps for itself ([`Keymap::locked`]), every name an i18n key
//! part, no chord twice.

use std::fmt;

use super::{Chord, Keymap, KeymapError, is_name};

/// The most deviations the setting holds.
pub const MAX_OVERRIDES: usize = 256;
/// The longest text of the setting, in characters (an export's longest field).
pub const MAX_OVERRIDE_TEXT: usize = 8192;

/// One deviation from the standard keymap.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Override {
    /// `chord` runs the standard binding named `name`.
    Bind { chord: Chord, name: String },
    /// `chord` runs nothing, though the standard binds it.
    Unbind { chord: Chord },
}

impl Override {
    /// The chord it is about.
    #[must_use]
    pub fn chord(&self) -> &Chord {
        match self {
            Self::Bind { chord, .. } | Self::Unbind { chord } => chord,
        }
    }
}

impl fmt::Display for Override {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Bind { chord, name } => write!(formatter, "{chord}={name}"),
            Self::Unbind { chord } => write!(formatter, "{chord}="),
        }
    }
}

/// The deviations, in the order they apply.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Overrides(Vec<Override>);

impl Overrides {
    /// The deviations the setting's text names.
    ///
    /// # Errors
    ///
    /// The text is too long or holds too many words, a word is no `<chord>=<name>` or
    /// `<chord>=`, a chord is locked, or a chord is named twice.
    pub fn parse(text: &str) -> Result<Self, KeymapError> {
        if text.chars().count() > MAX_OVERRIDE_TEXT {
            return Err(KeymapError::TooLong);
        }
        let mut rows: Vec<Override> = Vec::new();
        for word in text.split_whitespace() {
            // A name never holds `=`; a chord may end in it (`Ctrl+=`).
            let wrong = || KeymapError::Override(word.to_owned());
            let (keys, name) = word.rsplit_once('=').ok_or_else(wrong)?;
            let chord: Chord = keys.parse().map_err(|_| wrong())?;
            if Keymap::locked(&chord).is_some() {
                return Err(KeymapError::Locked {
                    keys: chord.to_string(),
                });
            }
            if rows.iter().any(|row| *row.chord() == chord) {
                return Err(KeymapError::Duplicate {
                    keys: chord.to_string(),
                });
            }
            let row = match name {
                "" => Override::Unbind { chord },
                name if is_name(name) => Override::Bind {
                    chord,
                    name: name.to_owned(),
                },
                _ => return Err(wrong()),
            };
            rows.push(row);
            if rows.len() > MAX_OVERRIDES {
                return Err(KeymapError::TooLong);
            }
        }
        Ok(Self(rows))
    }

    /// The deviations in their order.
    #[must_use]
    pub fn rows(&self) -> &[Override] {
        &self.0
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// What turns `standard` into `effective`: an unbinding for every standard chord that runs
    /// nothing any more, a binding for every chord that runs another row than in the standard
    /// or none there. Window keys never change ([`Keymap::locked`]).
    #[must_use]
    pub fn between(standard: &Keymap, effective: &Keymap) -> Self {
        let mut rows = Vec::new();
        for kept in standard.bindings() {
            if !effective.bindings().iter().any(|b| b.chord == kept.chord) {
                rows.push(Override::Unbind {
                    chord: kept.chord.clone(),
                });
            }
        }
        for binding in effective.bindings() {
            let standard_row = standard
                .bindings()
                .iter()
                .any(|kept| kept.chord == binding.chord && kept.name == binding.name);
            if !standard_row {
                rows.push(Override::Bind {
                    chord: binding.chord.clone(),
                    name: binding.name.clone(),
                });
            }
        }
        Self(rows)
    }
}

impl fmt::Display for Overrides {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (at, row) in self.0.iter().enumerate() {
            if at > 0 {
                formatter.write_str(" ")?;
            }
            write!(formatter, "{row}")?;
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "overrides_tests.rs"]
mod tests;
