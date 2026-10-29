//! The settings' Plugins page (RC-100-09, `docs/plugins.md` "Installation and lifecycle"): the
//! installed plugins with their versions, switch, health, key and log; "Install..." with the
//! preview of a package before anything is written; Enable, Disable, Roll back, Resume, Remove;
//! and a plugin's own `[[settings]]` - the page's tab "Installed". The master switch, the crash
//! limit, the output limit, the token colours, the associations and development mode are schema
//! rows on its tab "Settings" (RC-100-20, `tabs.rs`); a package's preview turns to "Installed".
//!
//! Every action is a registry command run as a [`PluginsCall`] - `plugins.list`, `.preview`,
//! `.install`, `.enable`, `.disable`, `.rollback`, `.resume`, `.remove` - except a plugin's own
//! setting, which only the window sets (`Engine::set_plugin_setting`). An unknown signing key is
//! trusted only by installing from the preview that showed its fingerprint.

use std::collections::BTreeMap;

use rc_db::PluginExecution;
use rc_engine::{Command, Engine, KeyStatus, Outcome, PackagePreview, PluginInfo, PluginsOutcome};

use super::{Page, SettingsMessage, Tab, wrap};
use crate::{
    app::{App, Effect, Message},
    i18n::{error_text, t},
};

/// A package's preview, shown before anything is installed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Preview {
    pub path: String,
    /// The SHA-256 of the file the preview read; Install names it, so the engine refuses a
    /// package swapped since (RC-080-53).
    pub digest: String,
    pub preview: PackagePreview,
    pub installable: bool,
    pub installed_versions: Vec<String>,
    pub added_permissions: Vec<String>,
}

/// What the page keeps while the dialog is open.
#[derive(Debug, Default)]
pub struct PluginsPage {
    /// The installed plugins by name; `None` until read.
    pub plugins: Option<Vec<PluginInfo>>,
    /// The plugin whose details - versions, log, settings - are shown.
    pub selected: Option<String>,
    /// Its latest calls, newest first.
    pub executions: Vec<PluginExecution>,
    /// The package path, typed or chosen with "Browse...".
    pub path: String,
    /// The preview of the package at `path`, while it is open.
    pub preview: Option<Box<Preview>>,
    /// The selected plugin's own settings as edited, by name.
    pub drafts: BTreeMap<String, String>,
    /// The version of the selected plugin Remove takes.
    pub version: Option<String>,
    /// A command runs; the page's buttons wait.
    pub busy: bool,
    /// The secret sources among them and whether each answers (RC-100-36).
    pub sources: Vec<rc_engine::secret_sources::SecretSourceInfo>,
}

impl PluginsPage {
    pub fn plugin(&self, id: &str) -> Option<&PluginInfo> {
        self.plugins
            .as_deref()?
            .iter()
            .find(|plugin| plugin.id == id)
    }

    fn selected_plugin(&self) -> Option<&PluginInfo> {
        self.plugin(self.selected.as_deref()?)
    }
}

#[derive(Debug, Clone)]
pub enum PluginsMsg {
    /// The package path, typed or chosen.
    Path(String),
    /// "Install...": the preview of the package at the path.
    Preview,
    /// Install from the preview, trusting the key it showed.
    Install,
    /// Close the preview without installing.
    CancelPreview,
    Select(String),
    /// Switch a plugin on (`true`) or off.
    Enable(String, bool),
    Rollback(String),
    Resume(String),
    /// The version of the selected plugin Remove takes.
    Version(String),
    /// Remove the chosen version of the selected plugin.
    Remove,
    /// A plugin setting of the selected plugin, as edited.
    Setting(String, String),
    /// Store the selected plugin's edited settings.
    SaveSettings,
    Answered(Box<PluginsAnswer>),
}

pub fn page_message(message: PluginsMsg) -> Message {
    wrap(SettingsMessage::Plugins(message))
}

/// A call for the engine; its answer comes back to the page.
#[derive(Debug)]
pub enum PluginsCall {
    List {
        selected: Option<String>,
    },
    Preview(String),
    /// A command that changes a plugin; the note to show when it worked, in the person's
    /// language.
    Change(Box<Command>, &'static str),
    /// A plugin's own settings, `(name, value)` each.
    Settings {
        id: String,
        values: Vec<(String, String)>,
    },
}

/// The listing: plugins, the selected one's calls, the secret sources' states.
type Listed = (
    Vec<PluginInfo>,
    Vec<PluginExecution>,
    Vec<rc_engine::secret_sources::SecretSourceInfo>,
);

/// What became of a [`PluginsCall`].
#[derive(Debug, Clone)]
pub enum PluginsAnswer {
    Listed(Result<Listed, &'static str>),
    Previewed(Result<Box<Preview>, &'static str>),
    /// The note, or the refusal's code.
    Changed(Result<&'static str, &'static str>),
}

impl PluginsCall {
    pub async fn run(self, engine: Engine) -> Message {
        let answer = match self {
            Self::List { selected } => PluginsAnswer::Listed(list(&engine, selected).await),
            Self::Preview(path) => PluginsAnswer::Previewed(
                match dispatch(&engine, Command::PreviewPlugin { path }).await {
                    Ok(PluginsOutcome::Preview {
                        path,
                        digest,
                        preview,
                        installable,
                        installed_versions,
                        added_permissions,
                    }) => Ok(Box::new(Preview {
                        path,
                        digest,
                        preview,
                        installable,
                        installed_versions,
                        added_permissions,
                    })),
                    Ok(_) => Err("plugin_invalid"),
                    Err(code) => Err(code),
                },
            ),
            Self::Change(command, note) => {
                PluginsAnswer::Changed(dispatch(&engine, *command).await.map(|_| note))
            }
            Self::Settings { id, values } => {
                let mut saved = Ok(t!("settings.plugins.setting_saved"));
                for (name, value) in values {
                    if let Err(error) = engine.set_plugin_setting(&id, &name, &value).await {
                        saved = Err(error.code());
                        break;
                    }
                }
                PluginsAnswer::Changed(saved)
            }
        };
        page_message(PluginsMsg::Answered(Box::new(answer)))
    }
}

async fn dispatch(engine: &Engine, command: Command) -> Result<PluginsOutcome, &'static str> {
    match engine.dispatch(command).await {
        Ok(Outcome::Plugins(outcome)) => Ok(*outcome),
        Ok(_) => Err("plugin_invalid"),
        Err(error) => Err(error.code()),
    }
}

/// Every plugin, and the calls of the selected one when it is still installed.
async fn list(engine: &Engine, selected: Option<String>) -> Result<Listed, &'static str> {
    let PluginsOutcome::List { plugins, .. } =
        dispatch(engine, Command::ListPlugins { id: None }).await?
    else {
        return Err("plugin_invalid");
    };
    let executions = match selected.filter(|id| plugins.iter().any(|plugin| plugin.id == *id)) {
        Some(id) => match dispatch(engine, Command::ListPlugins { id: Some(id) }).await? {
            PluginsOutcome::List { executions, .. } => executions,
            _ => Vec::new(),
        },
        None => Vec::new(),
    };
    // A source that cannot be asked is only not marked.
    let sources = match engine
        .dispatch(Command::ListSecretSources {
            connection_id: None,
        })
        .await
    {
        Ok(Outcome::SecretSources(outcome)) => match *outcome {
            rc_engine::SecretSourcesOutcome::Sources { sources, .. } => sources,
            _ => Vec::new(),
        },
        _ => Vec::new(),
    };
    Ok((plugins, executions, sources))
}

fn call(call: PluginsCall) -> Vec<Effect> {
    vec![Effect::Plugins(call)]
}

impl App {
    /// The page was turned to: its list is read (again).
    pub(super) fn plugins_shown(&mut self) -> Vec<Effect> {
        let Some(dialog) = self.settings.as_mut() else {
            return Vec::new();
        };
        if dialog.page != Page::Plugins {
            return Vec::new();
        }
        let selected = dialog.plugins.selected.clone();
        call(PluginsCall::List { selected })
    }

    /// A message of the page.
    pub(super) fn plugins_message(&mut self, message: PluginsMsg) -> Vec<Effect> {
        // The preview is shown on "Installed", whichever tab was chosen.
        if let PluginsMsg::Answered(answer) = &message
            && matches!(**answer, PluginsAnswer::Previewed(Ok(_)))
        {
            self.settings_tabs.choose(Tab::Installed);
        }
        let Some(dialog) = self.settings.as_mut() else {
            return Vec::new();
        };
        let page = &mut dialog.plugins;
        let change = |page: &mut PluginsPage, command: Command, note: &'static str| {
            page.busy = true;
            call(PluginsCall::Change(Box::new(command), note))
        };
        match message {
            PluginsMsg::Path(path) => page.path = path,
            PluginsMsg::Preview => {
                let path = page.path.trim().to_owned();
                if !page.busy && !path.is_empty() {
                    page.busy = true;
                    dialog.note = None;
                    return call(PluginsCall::Preview(path));
                }
            }
            PluginsMsg::CancelPreview => page.preview = None,
            PluginsMsg::Install => {
                let Some(shown) = page.preview.as_ref().filter(|shown| shown.installable) else {
                    return Vec::new();
                };
                // The fingerprint the person saw, for a key nobody trusted yet.
                let fingerprint = (shown.preview.key == KeyStatus::Untrusted)
                    .then(|| shown.preview.fingerprint.clone())
                    .flatten();
                let command = Command::InstallPlugin {
                    path: shown.path.clone(),
                    fingerprint,
                    digest: shown.digest.clone(),
                };
                let id = shown.preview.id.clone();
                if !page.busy {
                    page.selected = Some(id);
                    return change(page, command, t!("settings.plugins.installed"));
                }
            }
            PluginsMsg::Select(id) => {
                page.selected = Some(id.clone());
                page.drafts.clear();
                page.version = None;
                return call(PluginsCall::List { selected: Some(id) });
            }
            PluginsMsg::Enable(id, on) if !page.busy => {
                let (command, note) = if on {
                    (Command::EnablePlugin { id }, t!("settings.plugins.enabled"))
                } else {
                    (
                        Command::DisablePlugin { id },
                        t!("settings.plugins.disabled"),
                    )
                };
                return change(page, command, note);
            }
            PluginsMsg::Rollback(id) if !page.busy => {
                return change(
                    page,
                    Command::RollbackPlugin { id },
                    t!("settings.plugins.rolled_back"),
                );
            }
            PluginsMsg::Resume(id) if !page.busy => {
                return change(
                    page,
                    Command::ResumePlugin { id },
                    t!("settings.plugins.resumed"),
                );
            }
            PluginsMsg::Version(version) => page.version = Some(version),
            PluginsMsg::Remove if !page.busy => {
                let (Some(id), Some(version)) = (page.selected.clone(), page.version.take()) else {
                    return Vec::new();
                };
                return change(
                    page,
                    Command::RemovePlugin { id, version },
                    t!("settings.plugins.removed"),
                );
            }
            PluginsMsg::Setting(name, value) => {
                page.drafts.insert(name, value);
            }
            PluginsMsg::SaveSettings if !page.busy => {
                let Some(id) = page.selected_plugin().map(|plugin| plugin.id.clone()) else {
                    return Vec::new();
                };
                // Kept until the plugin took them: a refusal leaves what was typed (RC-080-44).
                let values = page.drafts.clone().into_iter().collect();
                page.busy = true;
                return call(PluginsCall::Settings { id, values });
            }
            PluginsMsg::Enable(..)
            | PluginsMsg::Rollback(_)
            | PluginsMsg::Resume(_)
            | PluginsMsg::Remove
            | PluginsMsg::SaveSettings => {}
            PluginsMsg::Answered(answer) => match *answer {
                PluginsAnswer::Listed(Ok((plugins, executions, sources))) => {
                    page.sources = sources;
                    page.selected = page
                        .selected
                        .take()
                        .filter(|id| plugins.iter().any(|plugin| plugin.id == *id));
                    page.executions = executions;
                    page.plugins = Some(plugins);
                }
                PluginsAnswer::Previewed(Ok(preview)) => {
                    page.busy = false;
                    page.preview = Some(preview);
                }
                PluginsAnswer::Changed(Ok(note)) => {
                    page.busy = false;
                    page.preview = None;
                    page.drafts.clear();
                    dialog.note = Some((note.to_owned(), false));
                    let selected = page.selected.clone();
                    return call(PluginsCall::List { selected });
                }
                PluginsAnswer::Listed(Err(code))
                | PluginsAnswer::Previewed(Err(code))
                | PluginsAnswer::Changed(Err(code)) => {
                    page.busy = false;
                    dialog.note = Some((error_text(code), true));
                }
            },
        }
        Vec::new()
    }
}

#[cfg(test)]
#[path = "plugins_tests.rs"]
mod tests;
