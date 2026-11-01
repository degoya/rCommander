//! The colour themes of a connection (RC-100-51): `theme_light` names a light theme this system
//! offers, `theme_dark` a dark one, or none - the window's applies. Every way in - the window's
//! form, `connections.create`, `connections.update`, `connections.save`, MCP - is checked here
//! before the write; the repository checks only the id's length and characters. The themes of
//! the WSL distributions (`appearance.wsl_themes`, RC-100-56) are checked the same way before
//! `settings.set` stores them; the schema checks only the text's shape.

use serde_json::Value;

use crate::{
    Engine, EngineError,
    settings::{
        parse_wsl_themes,
        themes::{ThemeMode, ThemesState, load},
    },
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

    /// The themes `value`, a checked `appearance.wsl_themes`, gives the distributions, checked
    /// against those this system offers; a text that names none reads nothing.
    pub(crate) async fn check_wsl_themes(&self, value: &Value) -> Result<(), EngineError> {
        let rows = parse_wsl_themes(value.as_str().unwrap_or_default()).unwrap_or_default();
        if rows.is_empty() {
            return Ok(());
        }
        let themes = load(self.db()?).await;
        for row in &rows {
            check_theme(&themes, row.light.as_deref(), ThemeMode::Light)?;
            check_theme(&themes, row.dark.as_deref(), ThemeMode::Dark)?;
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "connections_themes_tests.rs"]
mod tests;
