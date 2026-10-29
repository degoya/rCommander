//! `panel.filter`, Total Commander's quick filter: Ctrl+S shows only the entries whose names
//! match a typed text ([`NameMatch::filter`]), Ctrl+Shift+S applies the panel's last filter
//! again, Escape or an empty text shows every entry.
//!
//! The filter belongs to the panel's state ([`QuickFilter`]), so it holds for the panel (and,
//! with tabs, for its tab) until it is lifted or the panel shows another directory; a refresh,
//! a sort and the hidden-files switch keep it. While it is in force, `entries` are the matching
//! part of the listing: the cursor, the selection and every file operation see only those, so a
//! hidden entry is never selected nor copied unseen - a filter drops it from the selection.

use std::sync::Arc;

use rc_vfs::Entry;

use crate::{
    Engine, EngineError, EngineEvent, Outcome, PanelChange, PanelId, PanelState,
    quick_search::{MAX_PATTERN, NameMatch, filtered},
};

/// A panel's quick filter.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct QuickFilter {
    /// The text in force; `None` while every entry shows.
    pub pattern: Option<String>,
    /// The last text set in this panel, which Ctrl+Shift+S applies again; kept across folders.
    pub last: Option<String>,
    /// The whole listing in the panel's order while a text is in force.
    listed: Option<Arc<[Entry]>>,
}

impl PanelState {
    /// Every entry of the listing in the panel's order, whatever the filter shows.
    #[must_use]
    pub fn listed(&self) -> Arc<[Entry]> {
        self.filter
            .listed
            .clone()
            .unwrap_or_else(|| Arc::clone(&self.entries))
    }

    /// The filter `pattern` over the listing, or none for `None` or a blank text. The cursor
    /// stays on its entry when it is still shown, else goes to the first one; selected names no
    /// longer shown are deselected.
    pub(crate) fn set_filter(&mut self, pattern: Option<String>) {
        let listed = self.listed();
        let at = self.cursor_entry().map(|entry| entry.name.clone());
        match pattern.as_deref().and_then(NameMatch::filter) {
            Some(matcher) => {
                self.entries = filtered(&listed, &matcher).into();
                self.selection.retain(|name| matcher.matches(name));
                self.filter.listed = Some(listed);
                self.filter.last.clone_from(&pattern);
                self.filter.pattern = pattern;
            }
            None => {
                self.entries = listed;
                self.filter.listed = None;
                self.filter.pattern = None;
            }
        }
        self.cursor = at.and_then(|name| self.index_of(&name)).unwrap_or(0);
    }

    /// A new listing in the panel's order, filtered as the panel is.
    pub(crate) fn relist(&mut self, listed: Arc<[Entry]>) {
        match self.filter.pattern.as_deref().and_then(NameMatch::filter) {
            Some(matcher) => {
                self.entries = filtered(&listed, &matcher).into();
                self.filter.listed = Some(listed);
            }
            None => {
                self.entries = listed;
                self.filter.listed = None;
            }
        }
    }

    /// What this state keeps of `before`'s filter: the last text always, the text in force only
    /// in the same directory.
    pub(crate) fn carry_filter(&mut self, before: &PanelState) {
        self.filter.last.clone_from(&before.filter.last);
        if self.path == before.path
            && let Some(pattern) = &before.filter.pattern
        {
            self.filter.pattern = Some(pattern.clone());
            let listed = Arc::clone(&self.entries);
            self.relist(listed);
        }
    }
}

/// A filter text as far as it can be used: refused when too long or holding a control
/// character; a blank one is no filter.
fn checked(pattern: Option<&str>) -> Result<Option<String>, EngineError> {
    let Some(pattern) = pattern.map(str::trim).filter(|text| !text.is_empty()) else {
        return Ok(None);
    };
    if pattern.chars().count() > MAX_PATTERN {
        return Err(EngineError::FilterInvalid {
            reason: "longer than 256 characters",
        });
    }
    if pattern.chars().any(char::is_control) {
        return Err(EngineError::FilterInvalid {
            reason: "control characters",
        });
    }
    Ok(Some(pattern.to_owned()))
}

impl Engine {
    /// `panel.filter`: the panel shows only what `pattern` matches, or with `last` what its last
    /// filter matches (nothing changes when it had none); no pattern shows every entry.
    pub(crate) fn filter(
        &self,
        panel: Option<PanelId>,
        pattern: Option<&str>,
        last: bool,
    ) -> Result<Outcome, EngineError> {
        let id = self.resolve(panel);
        let pattern = if last { None } else { checked(pattern)? };
        let snapshot = {
            let mut state = self.state();
            let active = state.active == id;
            let panel = state.panel_mut(id);
            if last {
                if let Some(again) = panel.filter.last.clone() {
                    panel.set_filter(Some(again));
                }
            } else {
                panel.set_filter(pattern);
            }
            panel.snapshot(id, active)
        };
        self.publish(EngineEvent::Panel {
            panel: id,
            change: PanelChange::Listing,
        });
        Ok(Outcome::Panel(snapshot))
    }
}

#[cfg(test)]
#[path = "filter_tests.rs"]
mod tests;
