//! A button bar's buttons (RC-100-87) as the database keeps them: versioned JSON of
//! [`BarSpec`], each item a button or a separator. A button runs a command of the one command
//! model ([`CommandRef`], RC-100-86) or opens another bar as its sub-bar, by that bar's id; it
//! may name a symbol (a file and the index of an icon in it) and a tooltip. Everything here is
//! checked as untrusted input: it comes from the window, an import or the database.

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use super::{ButtonBarError, ButtonBarId};
use crate::user_commands::{CommandRef, MAX_FIELD};

/// The spec's version this build writes and reads; a newer one is refused, never guessed.
pub const SPEC_VERSION: u32 = 1;

/// The most items (buttons and separators) of one bar.
pub const MAX_BUTTONS: usize = 128;

/// How deep sub-bars nest below a bar the window shows.
pub const MAX_SUB_BAR_DEPTH: usize = 4;

/// The symbol of a button: a file (`.exe`, `.dll`, `.ico` on Windows; `.png`, `.ico`
/// everywhere) and the index of the icon in it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct IconRef {
    pub path: String,
    #[serde(default)]
    pub index: i64,
}

/// What a button does.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ButtonAction {
    /// Runs a command: a program, internal rows or an own command.
    Command(CommandRef),
    /// Opens the bar of this id as a menu below the button.
    SubBar(ButtonBarId),
}

/// One button.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BarButton {
    pub action: ButtonAction,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<IconRef>,
    /// What the pointer shows, and the button's text while it has no symbol; empty for the
    /// command's own name.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub tooltip: String,
}

/// One item of a bar.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BarItem {
    Button(BarButton),
    /// A gap between buttons.
    Separator,
}

impl BarItem {
    /// The button, when it is one.
    #[must_use]
    pub fn button(&self) -> Option<&BarButton> {
        match self {
            Self::Button(button) => Some(button),
            Self::Separator => None,
        }
    }
}

/// A bar's buttons in their order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BarSpec {
    pub version: u32,
    #[serde(default)]
    pub items: Vec<BarItem>,
}

impl Default for BarSpec {
    fn default() -> Self {
        Self {
            version: SPEC_VERSION,
            items: Vec::new(),
        }
    }
}

impl BarSpec {
    /// The spec read from the database's JSON.
    ///
    /// # Errors
    ///
    /// [`ButtonBarError::Invalid`]: no JSON of a spec, or one that does not fit
    /// ([`BarSpec::checked`]).
    pub fn from_json(text: &str) -> Result<Self, ButtonBarError> {
        let spec: Self = serde_json::from_str(text).map_err(|_| ButtonBarError::Invalid {
            reason: "the buttons are no spec of a bar".to_owned(),
        })?;
        spec.checked()
    }

    /// The JSON the database keeps.
    #[must_use]
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).expect("a spec of strings and numbers always serializes")
    }

    /// The ids of the sub-bars its buttons open, each once, in button order.
    #[must_use]
    pub fn sub_bars(&self) -> Vec<ButtonBarId> {
        let mut seen = HashSet::new();
        self.items
            .iter()
            .filter_map(BarItem::button)
            .filter_map(|button| match button.action {
                ButtonAction::SubBar(id) => Some(id),
                ButtonAction::Command(_) => None,
            })
            .filter(|id| seen.insert(*id))
            .collect()
    }

    /// The spec checked: its version, at most [`MAX_BUTTONS`] items, each command as an own
    /// command's is checked, tooltips and symbols without a control character and at most
    /// [`MAX_FIELD`] characters. Blanks around a tooltip go.
    ///
    /// # Errors
    ///
    /// [`ButtonBarError::Invalid`], naming what does not fit, never its text.
    pub fn checked(&self) -> Result<Self, ButtonBarError> {
        let invalid = |reason: String| ButtonBarError::Invalid { reason };
        if self.version != SPEC_VERSION {
            return Err(invalid(format!(
                "version {} is not the version {SPEC_VERSION} this build reads",
                self.version
            )));
        }
        if self.items.len() > MAX_BUTTONS {
            return Err(invalid(format!(
                "a bar holds at most {MAX_BUTTONS} buttons"
            )));
        }
        let mut items = Vec::with_capacity(self.items.len());
        for (at, item) in self.items.iter().enumerate() {
            let place = at + 1;
            items.push(match item {
                BarItem::Separator => BarItem::Separator,
                BarItem::Button(button) => {
                    check_text(place, "tooltip", &button.tooltip)?;
                    let action = match &button.action {
                        ButtonAction::Command(command) => {
                            ButtonAction::Command(command.checked().map_err(|_| {
                                invalid(format!("the command of button {place} does not fit"))
                            })?)
                        }
                        ButtonAction::SubBar(id) => ButtonAction::SubBar(*id),
                    };
                    let icon = match &button.icon {
                        Some(icon) if icon.path.trim().is_empty() => None,
                        Some(icon) => {
                            check_text(place, "symbol", &icon.path)?;
                            Some(IconRef {
                                path: icon.path.trim().to_owned(),
                                index: icon.index,
                            })
                        }
                        None => None,
                    };
                    BarItem::Button(BarButton {
                        action,
                        icon,
                        tooltip: button.tooltip.trim().to_owned(),
                    })
                }
            });
        }
        Ok(Self {
            version: SPEC_VERSION,
            items,
        })
    }
}

/// Refuses a field of the button at `place` with a control character or longer than
/// [`MAX_FIELD`] characters.
fn check_text(place: usize, field: &str, text: &str) -> Result<(), ButtonBarError> {
    if text.chars().any(char::is_control) || text.chars().count() > MAX_FIELD {
        return Err(ButtonBarError::Invalid {
            reason: format!(
                "the {field} of button {place} holds a control character or more than \
                 {MAX_FIELD} characters"
            ),
        });
    }
    Ok(())
}

/// A bar's name trimmed, when it has 1 to [`rc_db::MAX_BUTTON_BAR_NAME`] characters and no
/// control character.
///
/// # Errors
///
/// [`ButtonBarError::Name`].
pub fn clean_name(name: &str) -> Result<String, ButtonBarError> {
    let name = name.trim();
    let length = name.chars().count();
    if length == 0 || length > rc_db::MAX_BUTTON_BAR_NAME || name.chars().any(char::is_control) {
        return Err(ButtonBarError::Name);
    }
    Ok(name.to_owned())
}

/// Checks the sub-bars of every bar in `links` (a bar's id to the ids its buttons open): each
/// opens a bar that exists, none opens a bar above it, and none nests deeper than
/// [`MAX_SUB_BAR_DEPTH`] below any bar.
///
/// # Errors
///
/// [`ButtonBarError::Unknown`] for a sub-bar that is not there, [`ButtonBarError::Cycle`],
/// [`ButtonBarError::TooDeep`].
pub fn check_links(links: &HashMap<ButtonBarId, Vec<ButtonBarId>>) -> Result<(), ButtonBarError> {
    let mut ids: Vec<ButtonBarId> = links.keys().copied().collect();
    ids.sort_unstable();
    for id in ids {
        let mut path = vec![id];
        walk(links, &mut path)?;
    }
    Ok(())
}

/// Walks the sub-bars below the last bar of `path`, the bars above it in `path`.
fn walk(
    links: &HashMap<ButtonBarId, Vec<ButtonBarId>>,
    path: &mut Vec<ButtonBarId>,
) -> Result<(), ButtonBarError> {
    let Some(&last) = path.last() else {
        return Ok(());
    };
    let Some(children) = links.get(&last) else {
        return Err(ButtonBarError::Unknown { id: last });
    };
    for &child in children {
        if path.contains(&child) {
            return Err(ButtonBarError::Cycle);
        }
        if !links.contains_key(&child) {
            return Err(ButtonBarError::Unknown { id: child });
        }
        // `path` holds the top bar and the sub-bars down to here.
        if path.len() > MAX_SUB_BAR_DEPTH {
            return Err(ButtonBarError::TooDeep);
        }
        path.push(child);
        walk(links, path)?;
        path.pop();
    }
    Ok(())
}

#[cfg(test)]
#[path = "spec_tests.rs"]
mod tests;
