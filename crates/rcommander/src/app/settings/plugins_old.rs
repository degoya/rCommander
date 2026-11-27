//! "Remove old versions..." on the Plugins page (RC-110-55): every listing also asks the engine
//! which versions are old (`plugins.remove_old` without versions) - not running or not verifying -
//! so the button is pressed only when there are any. It opens a question on the page, as the Host
//! keys page asks before a removal: the versions that go with their plugin and why, those an open
//! viewer holds, which stay, and Remove and Cancel; Enter removes, Escape cancels. Remove names
//! the versions the question showed, and the engine removes only those still old; the dialog's
//! status line then says how many went.

use iced::{
    Element, Length,
    widget::{Column, column, container},
};
use rc_engine::{
    Chord, Command, Engine, OldReason, OldVersion, Outcome, PluginVersionRef, PluginsOutcome,
};

use super::{
    Page,
    plugins::{PluginsCall, PluginsMsg, PluginsPage, page_message},
    view::{hint, label},
};
use crate::{
    app::{App, Effect, Message},
    i18n::{error_text, t},
    theme::{self, Tokens, fit, icon::Icon, layout},
};

/// The columns' shares in the question: plugin, version, reason.
const PORTIONS: [u16; 3] = [5, 2, 5];

/// The old versions as the last listing found them, and the question.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OldVersions {
    /// What "Remove old versions..." removes.
    pub versions: Vec<OldVersion>,
    /// Old, but held by an open viewer: named in the question, left.
    pub held: Vec<OldVersion>,
    /// The question before the removal is open.
    pub asking: bool,
}

impl OldVersions {
    /// Whether the button can be pressed: there is something to remove, no command runs and the
    /// question is not open yet.
    pub fn offered(&self, busy: bool) -> bool {
        !busy && !self.asking && !self.versions.is_empty()
    }
}

/// A step of the question.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OldMsg {
    /// "Remove old versions...": the question opens.
    Ask,
    /// Remove, in the question.
    Confirm,
    /// Cancel, in the question: nothing is removed.
    Cancel,
}

/// The old versions without removing any; none when the engine cannot tell - the button then
/// stays unpressed.
pub(super) async fn listed(engine: &Engine) -> (Vec<OldVersion>, Vec<OldVersion>) {
    match engine
        .dispatch(Command::RemoveOldPlugins { versions: None })
        .await
    {
        Ok(Outcome::Plugins(outcome)) => match *outcome {
            PluginsOutcome::OldVersions { versions, held } => (versions, held),
            _ => (Vec::new(), Vec::new()),
        },
        _ => (Vec::new(), Vec::new()),
    }
}

/// Removes the confirmed `versions`: the note to show, or the refusal's code.
pub(super) async fn remove(
    engine: &Engine,
    versions: Vec<PluginVersionRef>,
) -> Result<String, &'static str> {
    match engine
        .dispatch(Command::RemoveOldPlugins {
            versions: Some(versions),
        })
        .await
    {
        Ok(Outcome::Plugins(outcome)) => match *outcome {
            PluginsOutcome::OldRemoved { removed, skipped } => Ok(note(&removed, &skipped)),
            _ => Err("plugin_invalid"),
        },
        Ok(_) => Err("plugin_invalid"),
        Err(error) => Err(error.code()),
    }
}

/// How many versions went, and which an open viewer held.
pub fn note(removed: &[OldVersion], skipped: &[OldVersion]) -> String {
    let note = t!("settings.plugins.old_removed", count = removed.len());
    if skipped.is_empty() {
        return note;
    }
    let held: Vec<String> = skipped.iter().map(named).collect();
    t!(
        "settings.plugins.old_skipped",
        note = note,
        versions = held.join(", ")
    )
}

/// The plugin and the version, as one line names them.
fn named(version: &OldVersion) -> String {
    format!("{} {}", version.name, version.version)
}

/// Why a version is old, in the person's language.
fn reason(version: &OldVersion) -> String {
    match (version.reason, version.refused.as_deref()) {
        (OldReason::Inactive, _) => t!("settings.plugins.old_inactive").to_owned(),
        (OldReason::Refused, Some("plugin_key_untrusted")) => {
            t!("settings.plugins.old_key_untrusted").to_owned()
        }
        (OldReason::Refused, Some(code)) => {
            t!("settings.plugins.old_refused", reason = error_text(code))
        }
        (OldReason::Refused, None) => t!("settings.plugins.old_unverified").to_owned(),
    }
}

/// A step of the question on `page`; the status line is `note`.
pub(super) fn update(
    page: &mut PluginsPage,
    note: &mut Option<(String, bool)>,
    message: OldMsg,
) -> Vec<Effect> {
    match message {
        OldMsg::Ask => {
            if page.old.offered(page.busy) {
                page.old.asking = true;
            }
        }
        OldMsg::Cancel => page.old.asking = false,
        OldMsg::Confirm => {
            if !page.old.asking || page.busy {
                return Vec::new();
            }
            page.old.asking = false;
            let versions: Vec<PluginVersionRef> = page
                .old
                .versions
                .iter()
                .map(|version| PluginVersionRef {
                    id: version.id.clone(),
                    version: version.version.clone(),
                })
                .collect();
            if versions.is_empty() {
                return Vec::new();
            }
            page.busy = true;
            *note = None;
            return vec![Effect::Plugins(PluginsCall::RemoveOld(versions))];
        }
    }
    Vec::new()
}

impl App {
    /// The page's keys while its question is open: Enter removes, Escape cancels; `None` for
    /// every other key, and while no question is open.
    pub(super) fn plugins_key(&mut self, chord: &Chord) -> Option<Vec<Effect>> {
        let dialog = self.settings.as_mut()?;
        if dialog.page != Page::Plugins || !dialog.plugins.old.asking || !chord.is_plain() {
            return None;
        }
        let message = match chord.key.as_str() {
            "Enter" => OldMsg::Confirm,
            "Escape" => OldMsg::Cancel,
            _ => return None,
        };
        Some(update(&mut dialog.plugins, &mut dialog.note, message))
    }
}

/// "Remove old versions...", pressed only with something to remove.
pub fn button<'a>(page: &PluginsPage) -> Element<'a, Message> {
    layout::secondary_icon_button(
        Icon::Delete,
        t!("settings.plugins.remove_old"),
        page.old
            .offered(page.busy)
            .then(|| page_message(PluginsMsg::Old(OldMsg::Ask))),
    )
}

/// The question before the removal: the versions that go as a table of plugin, version and
/// reason, those an open viewer holds, that it is for good, and Remove and Cancel.
pub fn question<'a>(old: &'a OldVersions, tokens: &'static Tokens) -> Element<'a, Message> {
    let header: Element<'a, Message> = fit::table_row(
        false,
        tokens,
        [
            t!("settings.plugins.old_plugin"),
            t!("settings.plugins.version"),
            t!("settings.plugins.old_reason"),
        ]
        .into_iter()
        .zip(PORTIONS)
        .map(|(title, portion)| fit::cell(title, portion, true, tokens))
        .collect(),
    );
    let [plugin, version, why] = PORTIONS;
    let rows: Column<'a, Message> =
        Column::with_children(old.versions.iter().enumerate().map(|(index, shown)| {
            let alternate = theme::alternate(index);
            container(fit::table_row(
                false,
                tokens,
                vec![
                    fit::cell(shown.name.clone(), plugin, false, tokens),
                    fit::cell(shown.version.clone(), version, false, tokens),
                    fit::cell(reason(shown), why, true, tokens),
                ],
            ))
            .padding([0, 6])
            .width(Length::Fill)
            .style(move |_| theme::striped_row(tokens, false, true, false, alternate))
            .into()
        }));
    let mut lines = column![
        label(t!(
            "settings.plugins.old_question",
            count = old.versions.len()
        )),
        container(header)
            .padding([0, 6])
            .style(move |_| theme::header(tokens)),
        rows,
    ]
    .spacing(4);
    if !old.held.is_empty() {
        let held: Vec<String> = old.held.iter().map(named).collect();
        lines = lines.push(hint(
            t!("settings.plugins.old_held", versions = held.join(", ")),
            tokens,
        ));
    }
    let buttons = layout::button_row(
        None,
        [
            layout::secondary_icon_button(
                Icon::Delete,
                t!("settings.plugins.old_confirm"),
                Some(page_message(PluginsMsg::Old(OldMsg::Confirm))),
            ),
            layout::secondary_icon_button(
                Icon::Close,
                t!("settings.plugins.old_cancel"),
                Some(page_message(PluginsMsg::Old(OldMsg::Cancel))),
            ),
        ],
        [Element::from(
            fit::prose(t!("settings.plugins.old_warning")).color(tokens.status.danger),
        )],
    );
    container(lines.push(buttons))
        .padding(10)
        .style(move |_| theme::header(tokens))
        .width(Length::Fill)
        .into()
}

#[cfg(test)]
#[path = "plugins_old_tests.rs"]
mod tests;
