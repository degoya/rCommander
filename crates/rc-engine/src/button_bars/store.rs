//! The stored button bars (RC-100-87): `button_bar.list`, `.get`, `.save`, `.rename`, `.delete`,
//! the import of a `.bar` file and `view.toggle_bar`, over `rc_db`'s `button_bars`. What goes in
//! is checked here as at any boundary - the name ([`clean_name`]), the buttons
//! ([`BarSpec::checked`]) and the sub-bars of every bar together ([`check_links`]): each exists,
//! none opens a bar above it, none nests deeper than four. What comes back is read as untrusted
//! too: a bar whose buttons do not read is listed without buttons, with a warning. Every change
//! announces [`EngineEvent::ButtonBarsChanged`].

use std::{collections::HashMap, path::PathBuf};

use rc_db::StoredButtonBar;
use serde_json::Value;

use super::{
    BarButton, BarItem, BarKind, BarSpec, ButtonAction, ButtonBar, ButtonBarError, ButtonBarId,
    ButtonBarsOutcome, ImportPreview, PreviewResult, answer, check_links, clean_name,
    error::store_error, import,
};
use crate::{
    Engine, EngineError, EngineEvent, Outcome, recover::LogPanic as _, settings::stored_bool,
};

/// The row `stored` as a bar: buttons that do not read leave it without any.
fn bar_of(stored: StoredButtonBar) -> ButtonBar {
    let spec = BarSpec::from_json(&stored.spec).unwrap_or_else(|_| {
        tracing::warn!(
            id = stored.id,
            "a stored button bar does not read and shows no buttons"
        );
        BarSpec::default()
    });
    ButtonBar {
        id: stored.id,
        name: stored.name,
        spec,
    }
}

/// Refuses a spec whose button names a symbol the collection does not have (RC-110-10); only on
/// saving, so a stored bar keeps reading when the collection loses a name.
fn check_symbols(spec: &BarSpec) -> Result<(), ButtonBarError> {
    for (at, item) in spec.items.iter().enumerate() {
        if let Some(icon) = item.button().and_then(|button| button.icon.as_ref()) {
            crate::user_commands::symbols::check(&icon.path).map_err(|reason| {
                ButtonBarError::Invalid {
                    reason: format!("button {}: {reason}", at + 1),
                }
            })?;
        }
    }
    Ok(())
}

/// The sub-bars every bar of `bars` opens, by id.
fn links_of(bars: &[ButtonBar]) -> HashMap<ButtonBarId, Vec<ButtonBarId>> {
    bars.iter()
        .map(|bar| (bar.id, bar.spec.sub_bars()))
        .collect()
}

/// A path to import from: absolute, without a control character.
fn import_path(path: &str) -> Result<PathBuf, EngineError> {
    let refused = || ButtonBarError::Import {
        reason: "the path is no absolute path of a file".to_owned(),
    };
    if path.trim().is_empty() || path.chars().any(char::is_control) {
        return Err(refused().into());
    }
    let path = PathBuf::from(path.trim());
    if !path.is_absolute() {
        return Err(refused().into());
    }
    Ok(path)
}

impl Engine {
    /// Every button bar with its buttons, by id.
    ///
    /// # Errors
    ///
    /// No database is attached, or the read fails.
    pub async fn button_bars(&self) -> Result<Vec<ButtonBar>, EngineError> {
        let stored = self.db()?.button_bars().await.map_err(store_error)?;
        Ok(stored.into_iter().map(bar_of).collect())
    }

    /// The button bar `id`.
    ///
    /// # Errors
    ///
    /// [`ButtonBarError::Unknown`], or the read fails.
    pub(crate) async fn button_bar(&self, id: ButtonBarId) -> Result<ButtonBar, EngineError> {
        let stored = self.db()?.button_bar(id).await.map_err(store_error)?;
        Ok(bar_of(stored.ok_or(ButtonBarError::Unknown { id })?))
    }

    /// `button_bar.list`.
    pub(crate) async fn list_button_bars(&self) -> Result<Outcome, EngineError> {
        let bars = self.button_bars().await?;
        Ok(answer(ButtonBarsOutcome::Bars { bars }))
    }

    /// `button_bar.get`.
    pub(crate) async fn get_button_bar(&self, id: ButtonBarId) -> Result<Outcome, EngineError> {
        let bar = self.button_bar(id).await?;
        Ok(answer(ButtonBarsOutcome::Bar { bar }))
    }

    /// `button_bar.save`.
    pub(crate) async fn save_button_bar(
        &self,
        id: Option<ButtonBarId>,
        name: &str,
        spec: &BarSpec,
    ) -> Result<Outcome, EngineError> {
        let id = self.store_button_bar(id, name, spec).await?;
        self.publish(EngineEvent::ButtonBarsChanged);
        Ok(answer(ButtonBarsOutcome::Saved { id }))
    }

    /// Stores `spec` under `name` as a new bar or over `id`, checked with the sub-bars of every
    /// bar; the id it has. Announces nothing: the caller does, once for an import.
    async fn store_button_bar(
        &self,
        id: Option<ButtonBarId>,
        name: &str,
        spec: &BarSpec,
    ) -> Result<ButtonBarId, EngineError> {
        let name = clean_name(name)?;
        let spec = spec.checked()?;
        check_symbols(&spec)?;
        let mut links = links_of(&self.button_bars().await?);
        if let Some(id) = id
            && !links.contains_key(&id)
        {
            return Err(ButtonBarError::Unknown { id }.into());
        }
        // A new bar is opened by none yet; its own sub-bars are checked under an id no bar has.
        let key = id.unwrap_or(ButtonBarId::MIN);
        links.insert(key, spec.sub_bars());
        check_links(&links)?;
        let stored = self
            .db()?
            .save_button_bar(id, &name, &spec.to_json())
            .await
            .map_err(store_error)?;
        Ok(stored.id)
    }

    /// `button_bar.rename`.
    pub(crate) async fn rename_button_bar(
        &self,
        id: ButtonBarId,
        name: &str,
    ) -> Result<Outcome, EngineError> {
        let name = clean_name(name)?;
        let stored = self
            .db()?
            .rename_button_bar(id, &name)
            .await
            .map_err(store_error)?;
        self.publish(EngineEvent::ButtonBarsChanged);
        Ok(answer(ButtonBarsOutcome::Saved { id: stored.id }))
    }

    /// `button_bar.delete`: refused while another bar opens it as its sub-bar.
    pub(crate) async fn delete_button_bar(&self, id: ButtonBarId) -> Result<Outcome, EngineError> {
        let bars = self.button_bars().await?;
        if !bars.iter().any(|bar| bar.id == id) {
            return Err(ButtonBarError::Unknown { id }.into());
        }
        if bars
            .iter()
            .any(|bar| bar.id != id && bar.spec.sub_bars().contains(&id))
        {
            return Err(ButtonBarError::InUse.into());
        }
        self.db()?
            .delete_button_bar(id)
            .await
            .map_err(store_error)?;
        self.publish(EngineEvent::ButtonBarsChanged);
        Ok(answer(ButtonBarsOutcome::Done {}))
    }

    /// `button_bar.import_preview`: the `.bar` file at `path` and its sub-bars, read off the
    /// async threads.
    pub(crate) async fn preview_button_bar_import(
        &self,
        path: &str,
    ) -> Result<Outcome, EngineError> {
        let preview = read_preview(import_path(path)?).await?;
        Ok(answer(ButtonBarsOutcome::Preview { preview }))
    }

    /// `button_bar.import`: the `.bar` file at `path` and its sub-bars as bars of their own, the
    /// sub-bars first; a name a bar has already gets ` (2)`, ` (3)`, ... What the preview skips
    /// is left out. A bar that fails takes the ones made before it with it.
    pub(crate) async fn import_button_bar(&self, path: &str) -> Result<Outcome, EngineError> {
        let preview = read_preview(import_path(path)?).await?;
        let mut taken: Vec<String> = self
            .button_bars()
            .await?
            .into_iter()
            .map(|bar| bar.name.to_lowercase())
            .collect();
        let mut made: Vec<Option<ButtonBarId>> = vec![None; preview.bars.len()];
        let mut order = Vec::new();
        for at in post_order(&preview) {
            let name = free_name(&preview.bars[at].name, &taken);
            let spec = spec_of(&preview, at, &made);
            match self.store_button_bar(None, &name, &spec).await {
                Ok(id) => {
                    made[at] = Some(id);
                    order.push(id);
                    taken.push(name.to_lowercase());
                }
                Err(error) => {
                    for id in order.into_iter().rev() {
                        if let Err(undo) = self.db()?.delete_button_bar(id).await {
                            tracing::warn!(id, %undo, "a bar of a failed import stays");
                        }
                    }
                    self.publish(EngineEvent::ButtonBarsChanged);
                    return Err(error);
                }
            }
        }
        self.publish(EngineEvent::ButtonBarsChanged);
        let bars = made.into_iter().flatten().collect();
        Ok(answer(ButtonBarsOutcome::Imported { bars }))
    }

    /// `view.toggle_bar`: the bar's setting turned over and announced, as `settings.set` does.
    pub(crate) async fn toggle_bar(&self, bar: BarKind) -> Result<Outcome, EngineError> {
        let key = bar.setting_key();
        let shown = stored_bool(self.db()?, key).await;
        self.set_setting(key, &Value::Bool(!shown)).await
    }
}

/// [`import::preview`] off the async threads.
async fn read_preview(path: PathBuf) -> Result<ImportPreview, EngineError> {
    tokio::task::spawn_blocking(move || import::preview(&path))
        .await
        .log_panic("engine.button_bar_import")
        .map_err(|_| ButtonBarError::Import {
            reason: "the file was not read".to_owned(),
        })?
        .map_err(Into::into)
}

/// The places of `preview`'s bars, each sub-bar before the bars that open it; the file's bar
/// last.
fn post_order(preview: &ImportPreview) -> Vec<usize> {
    fn visit(preview: &ImportPreview, at: usize, seen: &mut Vec<bool>, order: &mut Vec<usize>) {
        if seen[at] {
            return;
        }
        seen[at] = true;
        for item in &preview.bars[at].items {
            if let PreviewResult::SubBar { bar } = item.result
                && bar < preview.bars.len()
            {
                visit(preview, bar, seen, order);
            }
        }
        order.push(at);
    }
    let mut seen = vec![false; preview.bars.len()];
    let mut order = Vec::new();
    for at in 0..preview.bars.len() {
        visit(preview, at, &mut seen, &mut order);
    }
    order
}

/// The bar at `at` of `preview` as a spec: its commands, separators and sub-bars made already
/// (`made`); skipped buttons left out.
fn spec_of(preview: &ImportPreview, at: usize, made: &[Option<ButtonBarId>]) -> BarSpec {
    let items = preview.bars[at]
        .items
        .iter()
        .filter_map(|item| {
            let action = match &item.result {
                PreviewResult::Command { action, .. } => ButtonAction::Command(action.clone()),
                PreviewResult::SubBar { bar } => ButtonAction::SubBar(made.get(*bar).copied()??),
                PreviewResult::Separator => return Some(BarItem::Separator),
                PreviewResult::Skipped { .. } => return None,
            };
            Some(BarItem::Button(BarButton {
                action,
                icon: item.icon.clone(),
                tooltip: item.tooltip.clone(),
            }))
        })
        .collect();
    BarSpec {
        items,
        ..BarSpec::default()
    }
}

/// `name`, or `name (2)`, `name (3)`, ... when a bar of `taken` (lower case) has it; at most the
/// longest name a bar has.
fn free_name(name: &str, taken: &[String]) -> String {
    let free = |candidate: &str| !taken.contains(&candidate.to_lowercase());
    if free(name) {
        return name.to_owned();
    }
    (2..)
        .map(|number| {
            let suffix = format!(" ({number})");
            let room = rc_db::MAX_BUTTON_BAR_NAME.saturating_sub(suffix.chars().count());
            let stem: String = name.chars().take(room).collect();
            format!("{stem}{suffix}")
        })
        .find(|candidate| free(candidate))
        .unwrap_or_else(|| name.to_owned())
}

#[cfg(test)]
#[path = "store_tests.rs"]
mod tests;
