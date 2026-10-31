//! The colour themes of a connection (RC-100-51): `theme_light` names a light theme this system
//! offers, `theme_dark` a dark one, or none - the window's applies. Every way in - the window's
//! form, `connections.create`, `connections.update`, `connections.save`, MCP - is checked here
//! before the write; the repository checks only the id's length and characters.

use crate::{
    Engine, EngineError,
    settings::themes::{ThemeMode, ThemesState, load},
};

/// Refuses `id` for `mode` unless it names a theme of that mode `themes` offers; none or an
/// empty text (which clears) passes.
///
/// # Errors
///
/// [`EngineError::ThemeUnknown`], [`EngineError::ThemeWrongMode`].
pub(crate) fn check_theme(
    themes: &ThemesState,
    id: Option<&str>,
    mode: ThemeMode,
) -> Result<(), EngineError> {
    let Some(id) = id.map(str::trim).filter(|id| !id.is_empty()) else {
        return Ok(());
    };
    let theme = themes
        .theme(id)
        .ok_or_else(|| EngineError::ThemeUnknown { id: id.to_owned() })?;
    if theme.dark != mode.is_dark() {
        return Err(EngineError::ThemeWrongMode {
            id: id.to_owned(),
            mode: if mode.is_dark() { "dark" } else { "light" },
        });
    }
    Ok(())
}

impl Engine {
    /// The themes a write gives a connection, checked against those this system offers; a write
    /// that names none reads nothing.
    pub(super) async fn check_connection_themes(
        &self,
        light: Option<&str>,
        dark: Option<&str>,
    ) -> Result<(), EngineError> {
        if light.is_none() && dark.is_none() {
            return Ok(());
        }
        let themes = load(self.db()?).await;
        check_theme(&themes, light, ThemeMode::Light)?;
        check_theme(&themes, dark, ThemeMode::Dark)
    }
}

#[cfg(test)]
#[path = "connections_themes_tests.rs"]
mod tests;
