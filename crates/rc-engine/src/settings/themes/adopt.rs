//! The colours of RC-100-43 taken into themes (RC-100-50): the eight list colours a person chose
//! per look (`appearance.colours.<light|dark>.<token>`) become an own theme "Custom (light)" or
//! "Custom (dark)" with those values, which is chosen for its mode; the keys are removed then, so
//! they are read once. The window runs it at the start and whenever one of the keys is set again
//! (the import from Total Commander writes them, RC-050-14), so such colours land in the Custom
//! theme, updated in place. On macOS the Mac's look of RC-100-49 is carried over the same way:
//! while `appearance.macos_look` is on and a mode has no theme chosen yet, it gets the Mac theme.

use std::collections::BTreeMap;

use rc_db::{Db, DbError};
use serde_json::Value;
use tokio::sync::Mutex;

use super::{Theme, ThemeAccent, ThemeMode, clean_name, hex_colour, new_own_id, unique_name};
use crate::settings::{COLOUR_TOKENS, MACOS_LOOK_KEY, colour_key, colour_value, stored_bool};

/// The name of the theme the light look's chosen colours become.
pub const CUSTOM_LIGHT: &str = "Custom (light)";
/// The name of the theme the dark look's chosen colours become.
pub const CUSTOM_DARK: &str = "Custom (dark)";

/// One adoption at a time: an import sets the keys one after the other, and each change asks for
/// one; in turn, each finds the Custom theme the one before made.
static ADOPTING: Mutex<()> = Mutex::const_new(());

/// The fixed lists' fill of each mode (`theme/light.rs`, `theme/dark.rs`): the chosen text is
/// softened over it for the quieter text, as RC-100-43 drew it.
const BASE_LIST_FILL: [&str; 2] = ["#ffffff", "#272727"];

/// The colour chosen under `key`, when one is stored and reads.
async fn chosen(db: &Db, key: &str) -> Option<String> {
    let value = db.get::<Value>(key).await.ok().flatten()?;
    colour_value(value.as_str()?).filter(|colour| !colour.is_empty())
}

/// `top` over `base` at `share`, both `#rrggbb`.
fn mix(top: &str, base: &str, share: f32) -> Option<String> {
    let channel = |colour: &str, at: usize| u8::from_str_radix(colour.get(at..at + 2)?, 16).ok();
    let mut out = String::from("#");
    for at in [1, 3, 5] {
        let (high, low) = (f32::from(channel(top, at)?), f32::from(channel(base, at)?));
        let mixed = (high * share + low * (1.0 - share))
            .round()
            .clamp(0.0, 255.0) as u8;
        out.push_str(&format!("{mixed:02x}"));
    }
    Some(out)
}

/// The theme tokens and the accent the colours `values` (one per [`COLOUR_TOKENS`], `None` where
/// none is chosen) of the look `dark` make, as RC-100-43 laid them over the fixed look.
fn tokens_of(dark: bool, values: &[Option<String>]) -> (BTreeMap<String, String>, Option<String>) {
    let value = |token: &str| {
        let at = COLOUR_TOKENS.iter().position(|known| *known == token)?;
        values.get(at)?.clone()
    };
    let mut tokens = BTreeMap::new();
    let mut put = |token: &str, colour: Option<String>| {
        if let Some(colour) = colour {
            tokens.insert(token.to_owned(), colour);
        }
    };
    let background = value("background");
    if let Some(text) = value("text") {
        let base = background
            .clone()
            .unwrap_or_else(|| BASE_LIST_FILL[usize::from(dark)].to_owned());
        put("list_text_secondary", mix(&text, &base, 0.75));
        put("cursor_text", Some(text.clone()));
        put("list_text", Some(text));
    }
    put("list_fill", background);
    put("stripe_fill", value("stripe"));
    put("cursor_fill", value("cursor"));
    put("cursor_fill_inactive", value("cursor_inactive"));
    put("cursor_text", value("cursor_text"));
    put("marked", value("marked"));
    (tokens, value("accent"))
}

/// Takes the colours of RC-100-43 into the Custom themes and, on macOS, the Mac's look into the
/// modes without a theme; whether anything changed. The keys taken are removed.
///
/// # Errors
///
/// A read or a write of the database fails; what was written before stays.
pub async fn adopt_colours(db: &Db) -> Result<bool, DbError> {
    let _turn = ADOPTING.lock().await;
    let mut changed = false;
    for dark in [false, true] {
        let mut values = Vec::with_capacity(COLOUR_TOKENS.len());
        let mut keys = Vec::with_capacity(COLOUR_TOKENS.len());
        for token in COLOUR_TOKENS {
            let Some(key) = colour_key(dark, token) else {
                continue;
            };
            values.push(chosen(db, key).await);
            keys.push(key);
        }
        if values.iter().all(Option::is_none) {
            continue;
        }
        let (tokens, accent) = tokens_of(dark, &values);
        let name = if dark { CUSTOM_DARK } else { CUSTOM_LIGHT };
        let own: Vec<Theme> = db
            .themes()
            .await?
            .into_iter()
            .filter_map(Theme::from_stored)
            .collect();
        let mut theme = match own
            .iter()
            .find(|theme| theme.dark == dark && clean_name(&theme.name) == name)
        {
            Some(there) => there.clone(),
            None => Theme {
                id: new_own_id(),
                name: unique_name(name, own.iter().map(|theme| theme.name.as_str())),
                dark,
                accent: ThemeAccent::System,
                tokens: BTreeMap::new(),
                builtin: false,
            },
        };
        theme.tokens.extend(tokens);
        if let Some(accent) = accent.as_deref().and_then(hex_colour) {
            theme.accent = ThemeAccent::Colour(accent);
        }
        let saved = db.save_theme(&theme.to_stored()).await?;
        db.set(ThemeMode::of(dark).key(), &saved.id).await?;
        for key in keys {
            db.delete(key).await?;
        }
        tracing::info!(dark, "the chosen list colours became a Custom theme");
        changed = true;
    }
    if cfg!(target_os = "macos") && stored_bool(db, MACOS_LOOK_KEY).await {
        for (mode, id) in [
            (ThemeMode::Light, "mac-light"),
            (ThemeMode::Dark, "mac-dark"),
        ] {
            if db.get::<Value>(mode.key()).await?.is_none() {
                db.set(mode.key(), id).await?;
                changed = true;
            }
        }
    }
    Ok(changed)
}

#[cfg(test)]
#[path = "adopt_tests.rs"]
mod tests;
