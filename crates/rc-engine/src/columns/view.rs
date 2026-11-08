//! A panel's column view as the engine keeps it (RC-100-90): `panel.set_view` with a set or the
//! comments, `panel.toggle_comments` (Ctrl+Shift+F2, Total Commander's `cm_SrcComments`),
//! `panel.toggle_auto_columns` (`cm_ToggleAutoViewModeSwitch`), the rule read for a folder a
//! panel enters ([`FolderRule`]) and the folder's comments read while the mode shows them.

use std::sync::Arc;

use rc_vfs::{BackendId, VfsPath};
use serde_json::Value;

use super::{
    AUTO_DEFAULT_KEY, AUTO_SWITCH_KEY, AutoDefault, ColumnSetSpec, Field, FolderComments,
    FolderRule, matching_set,
};
use crate::{
    Engine, EngineError, EngineEvent, ListMode, Outcome, PanelChange, PanelId,
    commands::read_descriptions,
    settings::{stored_bool, stored_setting},
    shown_path,
};

impl Engine {
    /// What decides the mode of a panel entering `path`: the sets there are, the switch, the
    /// default and the set whose mask matches. Without a database nothing switches.
    pub(crate) async fn folder_rule(&self, path: &VfsPath) -> FolderRule {
        let Ok(db) = self.db() else {
            return FolderRule::default();
        };
        let Ok(sets) = self.column_sets().await else {
            return FolderRule::default();
        };
        let enabled = stored_bool(db, AUTO_SWITCH_KEY).await;
        let default = stored_setting(db, AUTO_DEFAULT_KEY)
            .await
            .as_ref()
            .and_then(Value::as_str)
            .map_or_else(AutoDefault::default, AutoDefault::from_setting);
        let matched = if enabled && sets.iter().any(|set| !set.spec.masks.is_empty()) {
            matching_set(&sets, &self.shown_location(path).await)
        } else {
            None
        };
        FolderRule {
            known: Some(sets.iter().map(|set| set.id).collect()),
            enabled,
            default,
            matched,
        }
    }

    /// `path` as the path line shows it: `sftp://<connection>/<path>` and `ftp://...` on a
    /// server, the archive file's path with the path inside appended in an archive.
    async fn shown_location(&self, path: &VfsPath) -> String {
        if matches!(path.backend, BackendId::Ftp { .. } | BackendId::Sftp { .. })
            && let Some((name, written)) = self.connection_writer(path).await
        {
            return written(&name, &path.path);
        }
        shown_path(path).display().to_string()
    }

    /// The columns `mode` shows: a set's (read from the database), the comments view's, none
    /// for the full and the brief view or a set that is gone.
    pub(crate) async fn mode_spec(&self, mode: ListMode) -> Option<ColumnSetSpec> {
        match mode {
            ListMode::Full | ListMode::Brief | ListMode::Thumbnails => None,
            ListMode::Comments => Some(ColumnSetSpec::comments()),
            ListMode::Custom { set } => self.read_column_set(set).await.ok().map(|set| set.spec),
        }
    }

    /// The folder's comments follow the mode of panel `id`: read (after the listing, outside the
    /// lock) when the mode shows a comment column and they are not there yet - or always with
    /// `reread` (F2, a comment set) - and dropped when it shows none. An unreadable
    /// `descript.ion` shows no comments. A panel that moved on meanwhile is left alone.
    pub(crate) async fn columns_follow(&self, id: PanelId, reread: bool) {
        let (mode, path, loaded) = {
            let state = self.state();
            let panel = state.panel(id);
            (
                panel.mode,
                panel.path.clone(),
                panel.columns.comments.is_some(),
            )
        };
        let wanted = self
            .mode_spec(mode)
            .await
            .is_some_and(|spec| spec.shows(Field::Comment));
        if !wanted {
            if loaded {
                let mut state = self.state();
                state.panel_mut(id).columns.comments = None;
            }
            return;
        }
        if loaded && !reread {
            return;
        }
        let comments = match self.vfs(&path) {
            Ok(vfs) => match read_descriptions(&vfs, &path).await {
                Ok(descriptions) => FolderComments::of(&descriptions),
                Err(error) => {
                    tracing::debug!(code = error.code(), "the folder's comments were not read");
                    FolderComments::default()
                }
            },
            Err(_) => FolderComments::default(),
        };
        {
            let mut state = self.state();
            let panel = state.panel_mut(id);
            if panel.path != path || panel.mode != mode {
                return;
            }
            panel.columns.comments = Some(Arc::new(comments));
            panel.order_by_comments();
        }
        self.publish(EngineEvent::Panel {
            panel: id,
            change: PanelChange::Listing,
        });
    }

    /// `panel.set_view`: full, brief, a column set (one that exists), the comments or the
    /// thumbnails, chosen by hand - it stays until the panel enters a folder of another set's
    /// mask.
    pub(crate) async fn set_list_mode(
        &self,
        panel: Option<PanelId>,
        mode: ListMode,
    ) -> Result<Outcome, EngineError> {
        if let ListMode::Custom { set } = mode {
            self.read_column_set(set).await?;
        }
        let id = self.resolve(panel);
        // A panel showing its tree shows its files again (RC-100-88).
        let outcome = self.show_mode(id, |_, _| mode).await?;
        Ok(self.leave_tree(outcome))
    }

    /// `panel.toggle_comments` (Ctrl+Shift+F2): the comments view, or back to the mode before it.
    pub(crate) async fn toggle_comments(
        &self,
        panel: Option<PanelId>,
    ) -> Result<Outcome, EngineError> {
        let id = self.resolve(panel);
        self.show_mode(id, |mode, before| match mode {
            ListMode::Comments => before.unwrap_or(ListMode::Full),
            _ => ListMode::Comments,
        })
        .await
    }

    /// Panel `id` in the mode `choose` gives for its mode and the mode before the comments, by
    /// hand; the comments follow.
    async fn show_mode(
        &self,
        id: PanelId,
        choose: impl FnOnce(ListMode, Option<ListMode>) -> ListMode,
    ) -> Result<Outcome, EngineError> {
        let snapshot = {
            let mut state = self.state();
            let active = state.active == id;
            let panel = state.panel_mut(id);
            let mode = choose(panel.mode, panel.columns.before_comments);
            panel.columns.before_comments = match mode {
                ListMode::Comments if panel.mode != ListMode::Comments => Some(panel.mode),
                ListMode::Comments => panel.columns.before_comments,
                _ => None,
            };
            panel.mode = mode;
            panel.columns.manual = true;
            panel.snapshot(id, active)
        };
        self.publish(EngineEvent::Panel {
            panel: id,
            change: PanelChange::Listing,
        });
        self.columns_follow(id, false).await;
        Ok(Outcome::Panel(snapshot))
    }

    /// `panel.toggle_auto_columns`: the automatic switch on or off, stored.
    pub(crate) async fn toggle_auto_columns(&self) -> Result<Outcome, EngineError> {
        let on = self.setting_on(AUTO_SWITCH_KEY).await;
        self.set_setting(AUTO_SWITCH_KEY, &Value::Bool(!on)).await
    }
}

#[cfg(test)]
#[path = "view_tests.rs"]
mod tests;
