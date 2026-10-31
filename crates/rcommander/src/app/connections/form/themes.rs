//! A connection's colour themes in the form (RC-100-51): Default - the window's - or a theme of
//! each mode, as the window knows them.

use super::{Field, Form};

/// The themes a connection can choose for the light or the `dark` mode, as the window knows them:
/// each id with the name it is shown under, the built-in ones first (RC-100-51). Default (the
/// window's theme) is not among them.
pub fn theme_choices(dark: bool) -> Vec<(String, String)> {
    crate::theme::themes::of_mode(dark)
        .into_iter()
        .map(|theme| {
            let name = crate::theme::builtin::name(&theme.id)
                .filter(|_| theme.builtin)
                .map_or_else(|| theme.name.clone(), str::to_owned);
            (theme.id, name)
        })
        .collect()
}

impl Form {
    /// The theme of the light or the `dark` mode, `None` for the window's.
    pub fn theme(&self, dark: bool) -> Option<&str> {
        if dark {
            self.theme_dark.as_deref()
        } else {
            self.theme_light.as_deref()
        }
    }

    /// Another theme for the light or the `dark` mode.
    pub fn set_theme(&mut self, dark: bool, theme: Option<String>) {
        let (slot, field) = if dark {
            (&mut self.theme_dark, Field::ThemeDark)
        } else {
            (&mut self.theme_light, Field::ThemeLight)
        };
        *slot = theme;
        self.touched(field);
    }
}
