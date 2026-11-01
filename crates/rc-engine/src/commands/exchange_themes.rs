//! The own colour themes of an export and an import (RC-100-50), part of the settings.
//!
//! An import checks every theme as the window's import of a `.rctheme` file does (the name
//! rule, a known token and `#rrggbb` each) and matches it by its id: new when the id is not here,
//! there already with the same name and colours, else changed. A theme whose name another own
//! theme has here comes in with a number after the name; a theme whose id is no own one's is not
//! imported. The themes are written before the settings that name them. A theme a connection or
//! a WSL distribution names that is neither here nor in the file is left out with a hint
//! (RC-100-51, RC-100-56).

use rc_db::Db;

use super::{
    exchange_import::{choose, written},
    exchange_import_rows::{refused, row},
};
use crate::{
    Engine, EngineError,
    exchange::{ExchangePart, ExchangeRow, ExportedTheme, RowStatus},
    settings::{
        parse_wsl_themes,
        themes::{
            Theme, ThemeAccent, ThemesState, clean_name, clean_tokens, is_own_id, unique_name,
        },
        write_wsl_themes,
    },
};

/// Whether the theme `id` of the light or the `dark` mode is here or among the file's
/// (`incoming`, the themes a dry run has not written yet).
pub(super) fn known_theme(
    here: &ThemesState,
    incoming: &[ExportedTheme],
    id: &str,
    dark: bool,
) -> bool {
    here.theme(id).is_some_and(|theme| theme.dark == dark)
        || incoming
            .iter()
            .any(|theme| theme.id == id && theme.dark == dark && is_own_id(id))
}

/// `text`, an `appearance.wsl_themes` its row has checked, without a theme that is not
/// [`known_theme`] for its mode, and whether one was left out: the setting comes in without it,
/// with a hint (RC-100-56).
pub(super) fn fit_wsl_themes(
    text: &str,
    here: &ThemesState,
    incoming: &[ExportedTheme],
) -> (String, bool) {
    let mut rows = parse_wsl_themes(text).unwrap_or_default();
    let mut dropped = false;
    for row in &mut rows {
        for (slot, dark) in [(&mut row.light, false), (&mut row.dark, true)] {
            if slot
                .as_deref()
                .is_some_and(|id| !known_theme(here, incoming, id, dark))
            {
                *slot = None;
                dropped = true;
            }
        }
    }
    (write_wsl_themes(&rows), dropped)
}

/// Every own theme, for an export with the settings.
pub(super) async fn export_themes(db: &Db) -> Vec<ExportedTheme> {
    let stored = db.themes().await.unwrap_or_else(|error| {
        tracing::warn!(%error, "the own themes cannot be read; the export holds none");
        Vec::new()
    });
    stored
        .into_iter()
        .filter_map(Theme::from_stored)
        .map(|theme| ExportedTheme {
            accent: theme.accent.as_text().to_owned(),
            id: theme.id,
            name: theme.name,
            dark: theme.dark,
            tokens: theme.tokens,
        })
        .collect()
}

/// `entry` as a theme, when it is one an own theme can be.
fn checked(entry: &ExportedTheme) -> Option<Theme> {
    let name = clean_name(&entry.name);
    let accent = ThemeAccent::parse(&entry.accent)?;
    let pairs = entry
        .tokens
        .iter()
        .map(|(token, value)| (token.as_str(), value.as_str()));
    let (tokens, _unknown) = clean_tokens(pairs).ok()?;
    (is_own_id(&entry.id) && !name.is_empty()).then(|| Theme {
        id: entry.id.clone(),
        name,
        dark: entry.dark,
        accent,
        tokens,
        builtin: false,
    })
}

impl Engine {
    /// The themes of an import against the own ones here; written unless `dry_run`.
    pub(super) async fn exchange_themes(
        &self,
        themes: &[ExportedTheme],
        only: Option<&[String]>,
        dry_run: bool,
    ) -> Result<Vec<ExchangeRow>, EngineError> {
        let db = self.db()?;
        let mut here: Vec<Theme> = db
            .themes()
            .await?
            .into_iter()
            .filter_map(Theme::from_stored)
            .collect();
        let mut rows = Vec::with_capacity(themes.len());
        for entry in themes {
            let mut row = row(
                format!("theme:{}", entry.id),
                ExchangePart::Settings,
                clean_name(&entry.name),
            );
            let Some(mut theme) = checked(entry) else {
                row.status = refused("theme_invalid");
                rows.push(row);
                continue;
            };
            match here.iter().find(|there| there.id == theme.id) {
                Some(there) if *there == theme => row.status = RowStatus::Exists,
                Some(there) => {
                    row.status = RowStatus::Changed;
                    row.old = Some(there.name.clone());
                }
                None => row.status = RowStatus::New,
            }
            row.new = Some(theme.name.clone());
            choose(&mut row, only);
            if !dry_run && row.selected {
                let others = here.iter().filter(|there| there.id != theme.id);
                theme.name = unique_name(&theme.name, others.map(|there| there.name.as_str()));
                let result = db.save_theme(&theme.to_stored()).await;
                let done = match row.status {
                    RowStatus::New => RowStatus::Created,
                    _ => RowStatus::Updated,
                };
                if result.is_ok() {
                    here.retain(|there| there.id != theme.id);
                    here.push(theme);
                }
                row.status = written(result.map(|_| ()).map_err(EngineError::from), done);
            }
            rows.push(row);
        }
        Ok(rows)
    }
}

#[cfg(test)]
#[path = "exchange_themes_tests.rs"]
mod tests;
