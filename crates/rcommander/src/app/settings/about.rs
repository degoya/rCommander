//! The settings' About page (RC-010-19), after rDownloader's: which build runs, where it comes
//! from, who made it, and under which licences the crates it contains are.
//!
//! Two tabs. *About* shows the logo, the build's facts, the addresses and the credits, all from
//! [`rc_engine::about::about`] - what `app.about` answers MCP, so the page and an agent never
//! disagree - and "Copy version info" puts one line for a bug report on the clipboard.
//! *Licences* shows rCommander's own licence and every crate the binary links with its licence,
//! from the list `scripts/licenses.sh` generates ([`licenses`]), with a filter and a count per
//! licence. An address the repository does not publish yet is text marked "not yet published",
//! never a link; a published one is copied with a click (nothing here starts a browser).

mod licenses;
mod logo;
mod view;

use rc_engine::about::{About, NAME, VERSION, author};

pub use self::logo::window_icon;
pub(super) use self::view::view;
use super::{SettingsMessage, wrap};
use crate::{
    app::{App, Effect, Message},
    i18n::t,
    theme::page_tabs,
};

/// The page's tabs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AboutTab {
    #[default]
    About,
    Licenses,
}

const TABS: [AboutTab; 2] = [AboutTab::About, AboutTab::Licenses];

/// What the page keeps while the dialog is open.
#[derive(Debug, Default)]
pub struct AboutPage {
    tab: AboutTab,
    /// The licence list's filter: a crate's name or licence contains it, ignoring case.
    filter: String,
    /// The update channel and the last check, once the self-update's state is read (RC-100-05).
    update_facts: Vec<(&'static str, String)>,
}

impl AboutPage {
    /// The update channel and the last check with its result, as the About page lists them.
    pub(in crate::app) fn set_update_facts(&mut self, status: &rc_engine::UpdateStatus) {
        let checked = match (&status.last_check, &status.last_code) {
            (None, _) => t!("settings.about.update_never").to_owned(),
            (Some(at), None) => at.format("%Y-%m-%d %H:%M UTC").to_string(),
            (Some(at), Some(code)) => format!(
                "{} ({})",
                at.format("%Y-%m-%d %H:%M UTC"),
                crate::i18n::error_text(code)
            ),
        };
        self.update_facts = vec![
            (t!("settings.about.update_channel"), status.channel.clone()),
            (t!("settings.about.update_checked"), checked),
        ];
    }

    /// Ctrl+Tab (`back`: Ctrl+Shift+Tab): the other tab (RC-100-20).
    pub(super) fn turn(&mut self, back: bool) {
        self.tab = page_tabs::turn(&TABS, self.tab, back);
    }
}

#[derive(Debug, Clone)]
pub enum AboutMsg {
    Tab(AboutTab),
    Filter(String),
    CopyVersion,
    /// A published address, to the clipboard.
    CopyLink(String),
}

fn about_message(message: AboutMsg) -> Message {
    wrap(SettingsMessage::About(message))
}

/// The window's title, fixed as Total Commander's: name, version and author, from the same
/// constants as the page and `app.about`.
pub fn window_title() -> String {
    t!(
        "app.title",
        name = NAME,
        version = VERSION,
        author = author()
    )
}

/// What the page shows: the engine's answer to `app.about`.
fn shown() -> About {
    rc_engine::about::about()
}

/// The build's facts as the page lists them: a label and its value.
fn facts(about: &About) -> [(&'static str, String); 4] {
    [
        (t!("settings.about.version"), about.version.clone()),
        (
            t!("settings.about.commit"),
            or_unknown(about.commit.as_deref()),
        ),
        (
            t!("settings.about.built"),
            or_unknown(about.built.as_deref()),
        ),
        (t!("settings.about.platform"), about.platform.clone()),
    ]
}

fn or_unknown(value: Option<&str>) -> String {
    value.unwrap_or(t!("settings.about.unknown")).to_owned()
}

/// The line "Copy version info" puts on the clipboard: name, version, commit, build time and
/// platform, for a bug report.
fn version_line(about: &About) -> String {
    t!(
        "settings.about.version_line",
        name = about.name,
        version = about.version,
        commit = or_unknown(about.commit.as_deref()),
        built = or_unknown(about.built.as_deref()),
        platform = about.platform,
    )
}

impl App {
    /// A message of the About page; it needs no database.
    pub(super) fn about_message(&mut self, message: AboutMsg) -> Vec<Effect> {
        let Some(dialog) = self.settings.as_mut() else {
            return Vec::new();
        };
        let copied = match message {
            AboutMsg::Tab(tab) => {
                dialog.about.tab = tab;
                return Vec::new();
            }
            AboutMsg::Filter(filter) => {
                dialog.about.filter = filter;
                return Vec::new();
            }
            AboutMsg::CopyVersion => {
                dialog.note = Some((t!("settings.about.version_copied").to_owned(), false));
                version_line(&shown())
            }
            AboutMsg::CopyLink(url) => {
                dialog.note = Some((t!("settings.about.link_copied").to_owned(), false));
                url
            }
        };
        vec![Effect::Task(iced::clipboard::write(copied))]
    }
}

#[cfg(test)]
#[path = "about_tests.rs"]
mod tests;
