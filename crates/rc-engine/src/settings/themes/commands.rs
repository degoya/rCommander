//! The `theme` commands (RC-100-50): `theme.list` and `theme.select`, which MCP reaches, and the
//! window's `theme.save` and `theme.delete` behind the Themes tab and its editor.
//!
//! `theme.select` stores the mode's setting through the settings' own path, so the window hears
//! of it as of any setting and draws with the theme at once. A save checks the draft as it would
//! a file: the name rule, a known token each, `#rrggbb` each, a name no other theme has; a
//! built-in theme is neither saved over nor deleted, and the theme a mode draws with is not
//! deleted before another is chosen.

use serde_json::Value;

use super::{
    Theme, ThemeAccent, ThemeDraft, ThemeMode, ThemesOutcome, builtin, clean_name, clean_tokens,
    is_own_id, load, new_own_id,
};
use crate::{Engine, EngineError, Outcome};

fn answer(state: &super::ThemesState, saved: Option<String>) -> Outcome {
    Outcome::Themes(Box::new(ThemesOutcome {
        themes: state.infos(),
        saved,
    }))
}

fn invalid(reason: impl Into<String>) -> EngineError {
    EngineError::ThemeInvalid {
        reason: reason.into(),
    }
}

/// The error for `id`, which names no own theme: built in, or none at all.
fn not_own(id: &str) -> EngineError {
    if builtin(id).is_some() {
        EngineError::ThemeBuiltin { id: id.to_owned() }
    } else {
        EngineError::ThemeUnknown { id: id.to_owned() }
    }
}

impl Engine {
    /// `theme.list`: every theme this system offers, the chosen ones marked.
    pub(crate) async fn list_themes(&self) -> Result<Outcome, EngineError> {
        let state = load(self.db()?).await;
        Ok(answer(&state, None))
    }

    /// `theme.select`: the theme `id` for `mode` from now on.
    pub(crate) async fn select_theme(
        &self,
        mode: ThemeMode,
        id: &str,
    ) -> Result<Outcome, EngineError> {
        let state = load(self.db()?).await;
        let theme = state
            .theme(id)
            .ok_or_else(|| EngineError::ThemeUnknown { id: id.to_owned() })?;
        if theme.dark != mode.is_dark() {
            return Err(EngineError::ThemeWrongMode {
                id: id.to_owned(),
                mode: if mode.is_dark() { "dark" } else { "light" },
            });
        }
        self.store_setting(mode.key(), &Value::from(id)).await?;
        let state = load(self.db()?).await;
        Ok(answer(&state, None))
    }

    /// `theme.save`: the draft checked and stored, new or over the own theme of its id.
    pub(crate) async fn save_theme(&self, draft: &ThemeDraft) -> Result<Outcome, EngineError> {
        let db = self.db()?;
        let name = clean_name(&draft.name);
        if name.is_empty() {
            return Err(invalid("a theme needs a name"));
        }
        let accent = ThemeAccent::parse(&draft.accent)
            .ok_or_else(|| invalid("the accent is neither system nor #rrggbb"))?;
        let (tokens, unknown) = clean_tokens(
            draft
                .tokens
                .iter()
                .map(|(token, value)| (token.as_str(), value.as_str())),
        )
        .map_err(|token| invalid(format!("{token} is no #rrggbb")))?;
        if let Some(token) = unknown.first() {
            return Err(invalid(format!("{token} is no token of a theme")));
        }
        let id = match &draft.id {
            Some(id) => {
                let there = if is_own_id(id) {
                    db.theme(id).await?
                } else {
                    None
                };
                let there = there.ok_or_else(|| not_own(id))?;
                if there.dark != draft.dark {
                    return Err(EngineError::ThemeWrongMode {
                        id: id.clone(),
                        mode: if there.dark { "dark" } else { "light" },
                    });
                }
                id.clone()
            }
            None => new_own_id(),
        };
        let taken = db.themes().await?.into_iter().any(|other| {
            other.id != id && clean_name(&other.name).to_lowercase() == name.to_lowercase()
        });
        if taken {
            return Err(EngineError::ThemeNameTaken { name });
        }
        let theme = Theme {
            id,
            name,
            dark: draft.dark,
            accent,
            tokens,
            builtin: false,
        };
        let saved = db.save_theme(&theme.to_stored()).await?;
        let state = load(db).await;
        Ok(answer(&state, Some(saved.id)))
    }

    /// `theme.delete`: the own theme `id` removed, unless a mode draws with it.
    pub(crate) async fn delete_theme(&self, id: &str) -> Result<Outcome, EngineError> {
        let db = self.db()?;
        if !is_own_id(id) {
            return Err(not_own(id));
        }
        let state = load(db).await;
        if state.chosen.iter().any(|chosen| chosen == id) {
            return Err(EngineError::ThemeInUse { id: id.to_owned() });
        }
        if !db.delete_theme(id).await? {
            return Err(EngineError::ThemeUnknown { id: id.to_owned() });
        }
        let state = load(db).await;
        Ok(answer(&state, None))
    }
}

#[cfg(test)]
#[path = "commands_tests.rs"]
mod tests;
