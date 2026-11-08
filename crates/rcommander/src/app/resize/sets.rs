//! The column sets the file lists draw (RC-100-90): read off the window's thread at the start,
//! after a change (`EngineEvent::ColumnSetsChanged`) and with the widths; a set's column dragged
//! in the header keeps its width in the set, written once the button goes up
//! (`column_sets.update`). A right click on a header opens the menu of the views
//! (`tabs/popup_views.rs`); its "Configure..." opens the settings' Columns tab.

use iced::Task;
use rc_engine::{
    ColumnSet, Command, PanelId,
    columns::{MAX_WIDTH, MIN_WIDTH},
};

use super::{Resize, ResizeMsg, wrap};
use crate::app::{
    App, Effect, Message,
    settings::{Page, SettingsMessage, Tab},
    tabs::popup::Popup,
};

impl Resize {
    /// The column set `id`, as last read.
    pub(in crate::app) fn set(&self, id: i64) -> Option<&ColumnSet> {
        self.sets.iter().find(|set| set.id == id)
    }

    /// The column sets as last read, in the person's order.
    pub(in crate::app) fn sets(&self) -> &[ColumnSet] {
        &self.sets
    }

    /// The width of the column `index` of the set `set`.
    pub(super) fn set_width(&self, set: i64, index: usize) -> Option<f32> {
        let column = self.set(set)?.spec.columns.get(index)?;
        Some(f32::from(column.width))
    }

    /// The column `index` of the set `set` `width` wide, held within a column's bounds.
    pub(super) fn set_set_width(&mut self, set: i64, index: usize, width: f32) {
        let Some(column) = self
            .sets
            .iter_mut()
            .find(|found| found.id == set)
            .and_then(|found| found.spec.columns.get_mut(index))
        else {
            return;
        };
        if width.is_finite() {
            let width = width
                .round()
                .clamp(f32::from(MIN_WIDTH), f32::from(MAX_WIDTH));
            // Within the bounds of a u16 by the clamp.
            column.width = width as u16;
        }
    }

    /// The width a new column of the field of the column `index` of the set `set` starts with.
    pub(super) fn default_width(&self, set: i64, index: usize) -> f32 {
        self.set(set)
            .and_then(|found| found.spec.columns.get(index))
            .map_or(f32::from(MIN_WIDTH), |column| {
                f32::from(column.field.default_width())
            })
    }
}

/// The column sets; none when they cannot be read.
async fn read(engine: rc_engine::Engine) -> Vec<ColumnSet> {
    engine.column_sets().await.unwrap_or_else(|error| {
        tracing::debug!(code = error.code(), "the column sets were not read");
        Vec::new()
    })
}

impl App {
    /// Reads the column sets off the window's thread.
    pub(in crate::app) fn column_sets_task(&self) -> Task<Message> {
        let engine = self.engine.clone();
        Task::perform(read(engine), |sets| wrap(ResizeMsg::Sets(sets)))
    }

    /// Keeps the widths of the set `set` as dragged: `column_sets.update` with its spec.
    pub(super) fn store_set(&self, set: i64) -> Vec<Effect> {
        let Some(found) = self.resize.set(set) else {
            return Vec::new();
        };
        let update = Command::UpdateColumnSet {
            id: set,
            spec: found.spec.clone(),
        };
        vec![Effect::Dispatch(self.active, vec![update])]
    }

    /// The menu of the views of `panel` (a right click on its column header).
    pub(super) fn column_sets_menu(&mut self, panel: PanelId) -> Vec<Effect> {
        let mode = self.pane(panel).state.mode;
        self.tabs.popup = Some(Popup::views(panel, self.resize.sets(), mode));
        Vec::new()
    }

    /// The settings on the Appearance page's Columns tab ("Configure..." of the views' menu).
    pub(in crate::app) fn configure_columns(&mut self) -> Vec<Effect> {
        let effects = self.settings_message(SettingsMessage::OpenPage(Page::Appearance));
        self.settings_tabs.choose(Tab::Columns);
        effects
    }
}

#[cfg(test)]
#[path = "sets_tests.rs"]
mod tests;
