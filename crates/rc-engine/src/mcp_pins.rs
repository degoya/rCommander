//! What an MCP question froze of the panels, and the check that the allowed call still finds
//! them so (RC-080-34).
//!
//! A question shows the panel a call acts on - its folder, its selection or the entry under its
//! cursor - and the call used to read them again only when it ran: while the person read, the
//! same agent could select everything (`panel_select`), move the cursor or change the folder,
//! and the person allowed something other than what ran. Now the gate freezes the panel before
//! it asks ([`Engine::freeze_mcp_panel`]): the question shows the frozen names, the call carries
//! them as explicit arguments, and it runs with the folders of the panels it reads pinned
//! ([`with_mcp_pins`]). A pinned panel found in another folder fails the call with
//! `panel_changed` ([`check`]): `Engine::dispatch_mcp` checks every pin before it dispatches, and
//! the commands that resolve names against a panel's folder check again under the lock they
//! read it with.

use std::future::Future;

use rc_vfs::VfsPath;

use super::{MAX_SHOWN_ENTRIES, McpPanelContext, readable};
use crate::{Engine, EngineError, PanelId, PanelState};

/// A panel's folder as the question showed it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpPin {
    pub panel: PanelId,
    pub path: VfsPath,
}

/// A panel as the gate froze it before asking.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpFrozenPanel {
    /// The panel the call acts on: the one it names, else the one active at that moment.
    pub panel: PanelId,
    /// Its folder.
    pub path: VfsPath,
    /// The other panel's folder: where a copy without a target goes.
    pub other: VfsPath,
    /// What a call without names takes, in full: the selection, else the entry under the
    /// cursor; with `cursor` that entry only.
    pub names: Vec<String>,
    /// The same as the question shows it.
    pub context: McpPanelContext,
}

tokio::task_local! {
    /// The panels the running MCP call was asked about.
    static PINS: Vec<McpPin>;
}

/// Runs `future`, one MCP tool call the person allowed, with `pins` held.
pub async fn with_mcp_pins<F: Future>(pins: Vec<McpPin>, future: F) -> F::Output {
    PINS.scope(pins, future).await
}

/// The folder the running call pinned `panel` to, if it did.
pub(crate) fn pinned(panel: PanelId) -> Option<VfsPath> {
    PINS.try_with(|pins| {
        pins.iter()
            .find(|pin| pin.panel == panel)
            .map(|pin| pin.path.clone())
    })
    .ok()
    .flatten()
}

/// Refuses with `panel_changed` when the running call pinned `panel` to another folder than
/// `path`, where it stands now; `Ok` for a panel it did not pin and outside such a call.
pub(crate) fn check(panel: PanelId, path: &VfsPath) -> Result<(), EngineError> {
    let moved = PINS
        .try_with(|pins| {
            pins.iter()
                .any(|pin| pin.panel == panel && pin.path != *path)
        })
        .unwrap_or(false);
    if moved {
        return Err(EngineError::PanelChanged {
            panel: match panel {
                PanelId::Left => "left",
                PanelId::Right => "right",
            },
        });
    }
    Ok(())
}

impl Engine {
    /// Runs `future`, an MCP tool call that judges where it writes before it dispatches, with
    /// both panels pinned to the folders they show now (RC-080-42); a pin the call holds already
    /// (the folder a question showed) stays. The guard resolves its places against these
    /// folders ([`Engine::resolve_target`], [`Engine::locate_target`]) and the command against
    /// the same ones, or it fails with `panel_changed`: what the guard judged is what is written.
    pub async fn pin_mcp_panels<F: Future>(&self, future: F) -> F::Output {
        let mut pins = PINS.try_with(Clone::clone).unwrap_or_default();
        {
            let state = self.state();
            for panel in PanelId::BOTH {
                if !pins.iter().any(|pin| pin.panel == panel) {
                    pins.push(McpPin {
                        panel,
                        path: state.panel(panel).path.clone(),
                    });
                }
            }
        }
        PINS.scope(pins, future).await
    }

    /// The folder `panel` stands in for the running MCP call: the one it pinned, else the
    /// current one. Outside an MCP call, the current one.
    #[must_use]
    pub fn mcp_folder(&self, panel: PanelId) -> VfsPath {
        pinned(panel).unwrap_or_else(|| self.state().panel(panel).path.clone())
    }

    /// [`check`] for both panels, as they stand now.
    pub(crate) fn check_mcp_pins(&self) -> Result<(), EngineError> {
        let state = self.state();
        PanelId::BOTH
            .into_iter()
            .try_for_each(|panel| check(panel, &state.panel(panel).path))
    }

    /// The panel `panel` (the active one when `None`) as an MCP question shows it and the call
    /// then carries it, read under one lock: its folder, the other panel's, and its selection
    /// or else the entry under its cursor - only that entry when the call takes the `cursor`.
    /// The names and the folder the question shows pass through [`readable`], as the arguments
    /// do.
    #[must_use]
    pub fn freeze_mcp_panel(&self, panel: Option<PanelId>, cursor: bool) -> McpFrozenPanel {
        let state = self.state();
        let id = panel.unwrap_or(state.active);
        let current = state.panel(id);
        let names: Vec<String> = if takes_selection(current, cursor) {
            current.selection.iter().cloned().collect()
        } else {
            current
                .cursor_entry()
                .map(|entry| entry.name.clone())
                .into_iter()
                .collect()
        };
        McpFrozenPanel {
            panel: id,
            path: current.path.clone(),
            other: state.panel(id.other()).path.clone(),
            names,
            context: context_of(id, current, cursor),
        }
    }

    /// The question's part of [`Engine::freeze_mcp_panel`] alone: no more names are read than
    /// it shows.
    pub(crate) fn mcp_panel_context(
        &self,
        panel: Option<PanelId>,
        cursor: bool,
    ) -> McpPanelContext {
        let state = self.state();
        let id = panel.unwrap_or(state.active);
        context_of(id, state.panel(id), cursor)
    }
}

/// Whether a question about `panel` is about its selection rather than the entry under its
/// cursor.
fn takes_selection(panel: &PanelState, cursor: bool) -> bool {
    !cursor && !panel.selection.is_empty()
}

/// Panel `id` as a question shows it: at most [`MAX_SHOWN_ENTRIES`] names, and how many more.
fn context_of(id: PanelId, panel: &PanelState, cursor: bool) -> McpPanelContext {
    let selection = takes_selection(panel, cursor);
    let (entries, total): (Vec<String>, usize) = if selection {
        let shown = panel.selection.iter().take(MAX_SHOWN_ENTRIES);
        (
            shown.map(|name| readable(name)).collect(),
            panel.selection.len(),
        )
    } else {
        let name = panel.cursor_entry().map(|entry| readable(&entry.name));
        let total = usize::from(name.is_some());
        (name.into_iter().collect(), total)
    };
    McpPanelContext {
        panel: id,
        path: readable(&panel.path.path.display().to_string()),
        entries,
        selection,
        more: total.saturating_sub(MAX_SHOWN_ENTRIES),
    }
}

#[cfg(test)]
#[path = "mcp_pins_tests.rs"]
mod tests;
