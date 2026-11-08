//! The dialog an `open` step opens that no step may open by its key: the settings, as they open
//! or on a page and a tab of it (RC-100-55) - `{ "open": "settings", "page": "appearance", "tab":
//! "themes" }` - and the workspaces' window - `{ "open": "workspaces" }`, whose key runs the
//! `GuiOnly` `workspaces.manager` (RC-100-68) -, and the own commands' window - `{ "open":
//! "commands" }`, as `usercmd.manager` opens it (RC-100-86). A page or a tab the settings do not
//! have, a tab of another page, a tab without its page, a page or tab that is no name, or a page
//! or tab given to the workspaces' or the own commands' window ends the run before a window
//! opens.

use anyhow::{Result, bail};
use serde::Deserialize;
use serde_json::Value;

use crate::app::settings::{Page, Tab, Tabs};

/// The dialog an `open` step names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RawOpening {
    Settings,
    Workspaces,
    Commands,
}

/// A dialog a step opens, checked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Opening {
    /// The settings, on `page` and `tab` when given; else on the page and tab they open on.
    Settings {
        page: Option<Page>,
        tab: Option<Tab>,
    },
    /// The workspaces' window, as Ctrl+Shift+W opens it.
    Workspaces,
    /// The own commands' window, as Configuration > "Own commands..." opens it.
    Commands,
}

/// The settings' pages by the name a scene gives them.
pub const PAGES: [(&str, Page); 14] = [
    ("appearance", Page::Appearance),
    ("folders", Page::Folders),
    ("keyboard", Page::Keyboard),
    ("terminal", Page::Terminal),
    ("vault", Page::Vault),
    ("host_keys", Page::HostKeys),
    ("retention", Page::Retention),
    ("mcp", Page::Mcp),
    ("programs", Page::Programs),
    ("transfers", Page::Transfers),
    ("plugins", Page::Plugins),
    ("import", Page::Import),
    ("updates", Page::Updates),
    ("about", Page::About),
];

/// The tabs of the settings' pages by the name a scene gives them.
pub const TABS: [(&str, Tab); 9] = [
    ("installed", Tab::Installed),
    ("plugin_settings", Tab::PluginSettings),
    ("window", Tab::Window),
    ("fonts", Tab::Fonts),
    ("lists", Tab::Lists),
    ("viewer", Tab::Viewer),
    ("themes", Tab::Themes),
    ("keys", Tab::Keys),
    ("shortcuts", Tab::Shortcuts),
];

/// `opening` with the step's `page` and `tab`, checked.
///
/// # Errors
///
/// A page or a tab is no name the settings have, a tab is not of the page, a tab comes
/// without its page, or the workspaces' or the own commands' window is given a page or a tab.
pub fn check(opening: RawOpening, page: Option<&Value>, tab: Option<&Value>) -> Result<Opening> {
    let window = match opening {
        RawOpening::Workspaces => Some(("workspaces'", Opening::Workspaces)),
        RawOpening::Commands => Some(("own commands'", Opening::Commands)),
        RawOpening::Settings => None,
    };
    if let Some((name, window)) = window {
        if page.is_some() || tab.is_some() {
            bail!("the {name} window has no pages or tabs");
        }
        return Ok(window);
    }
    let page = page.map(|page| named(page, "page", &PAGES)).transpose()?;
    let tab = tab.map(|tab| named(tab, "tab", &TABS)).transpose()?;
    match (page, tab) {
        (None, Some(_)) => bail!("a tab of the settings needs its page"),
        (Some(page), Some(tab)) if !on_page(page, tab) => {
            bail!("the settings' page {page:?} has no tab {tab:?}")
        }
        _ => Ok(Opening::Settings { page, tab }),
    }
}

/// The item `value` names among `names`.
fn named<T: Copy>(value: &Value, what: &str, names: &[(&str, T)]) -> Result<T> {
    let Some(name) = value.as_str() else {
        bail!("a {what} of the settings is a name");
    };
    match names.iter().find(|(known, _)| *known == name) {
        Some((_, item)) => Ok(*item),
        None => bail!("the settings have no {what} '{name}'"),
    }
}

/// Whether `tab` is one of `page`'s tabs.
fn on_page(page: Page, tab: Tab) -> bool {
    let mut tabs = Tabs::default();
    tabs.choose(tab);
    tabs.chosen(page) == Some(tab)
}

#[cfg(test)]
#[path = "opening_tests.rs"]
mod tests;
