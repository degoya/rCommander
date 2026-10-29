//! What the `plugins.*` commands answer: each plugin with its versions, switch, health, key and
//! settings, and the preview of a package before it is installed. Field names stay clear of the
//! MCP mask's secret names (`key`, `token`, ...), since `plugins_list` answers with them.

use rc_db::PluginExecution;
use rc_plugin_host::{PackagePreview, Permissions};
use serde::{Deserialize, Serialize};

/// One installed plugin as the Plugins page and `plugins_list` show it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginInfo {
    pub id: String,
    pub name: String,
    /// The plugin kind: `viewer`.
    pub kind: String,
    pub description: String,
    pub author: String,
    /// The version that runs; `None` when no version verifies.
    pub version: Option<String>,
    /// Switched on by the person (and the master switch on).
    pub enabled: bool,
    /// Suspended after repeated failures until the person resumes it.
    pub suspended: bool,
    /// It came with rCommander.
    pub bundled: bool,
    /// `false` for a package development mode accepted unsigned.
    pub signed: bool,
    /// The signing key's id and fingerprint.
    pub key_id: String,
    pub key_fingerprint: Option<String>,
    pub permissions: Permissions,
    /// Every installed version, newest first.
    pub versions: Vec<VersionInfo>,
    /// The plugin's own settings, `plugin.<id>.<name>`.
    pub settings: Vec<PluginSettingInfo>,
}

/// One installed version.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VersionInfo {
    pub version: String,
    /// `active` (it runs), `previous` (a rollback's target), `staged`, `pending` (it runs from
    /// the next start) or `refused` (it does not verify).
    pub state: String,
    /// Why a refused version does not verify: a stable code.
    pub refused: Option<String>,
    pub bundled: bool,
    pub signed: bool,
    /// An open viewer holds it: it cannot be removed now.
    pub in_use: bool,
}

/// One of a plugin's own settings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginSettingInfo {
    /// The rCommander setting: `plugin.<id>.<name>`.
    pub setting: String,
    /// The name the manifest declares; its label is the plugin's text `<id>.setting.<name>`.
    pub name: String,
    /// `choice` or `text`.
    #[serde(rename = "type")]
    pub kind: String,
    /// A choice's values.
    pub values: Vec<String>,
    pub multiline: bool,
    pub default: Option<String>,
    /// A key such as a licence key: hidden on the page, `[redacted]` over MCP (RC-100-36).
    #[serde(default)]
    pub masked: bool,
    /// The stored value; `None` while the default applies.
    pub value: Option<String>,
}

/// The answer of a `plugins.*` command.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "result", rename_all = "snake_case")]
pub enum PluginsOutcome {
    /// `plugins.list`: every plugin by name, or the one asked for with its latest calls.
    List {
        plugins: Vec<PluginInfo>,
        executions: Vec<PluginExecution>,
        /// The master switch and development mode.
        plugins_enabled: bool,
        development_mode: bool,
    },
    /// `plugins.preview`: what the package is, before anything is installed.
    Preview {
        path: String,
        /// The lower-case hex SHA-256 of the file this preview read: `plugins.install` names it,
        /// and a file that changed since is refused (`plugin_package_changed`).
        digest: String,
        preview: PackagePreview,
        /// Whether confirming can install it.
        installable: bool,
        /// The versions of this plugin installed already.
        installed_versions: Vec<String>,
        /// What it asks for beyond the version that runs.
        added_permissions: Vec<String>,
    },
    /// `plugins.install`: installed; a first install runs at once, an update from the next
    /// start.
    Installed {
        plugin: PluginInfo,
        version: String,
        runs_now: bool,
    },
    /// The plugin after `plugins.enable`, `.disable`, `.rollback` or `.resume`.
    Changed { plugin: PluginInfo },
    /// `plugins.remove`: the version removed.
    Removed { id: String, version: String },
}
